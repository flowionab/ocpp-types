mod batch;
mod csv;
mod eqcheck;
mod generics;
mod model;
mod naming;
mod pool;
mod rust;
mod schema;
mod standard;
mod validate;

use clap::Parser;
use model::OcppVersion;
use std::fs;
use std::path::Path;

#[derive(Parser)]
struct Args {
    /// A single schema file, or a directory of schema files to generate
    /// together (sharing definitions like `CustomDataType` into one
    /// `common.rs` instead of duplicating them per message). Either way,
    /// the schema directory name (`ocpp1.6j`, `ocpp2.0.1`, or `ocpp2.1`)
    /// must appear in the path, since wrapper primitives like `IdTag` are
    /// specific to one OCPP version.
    input: String,
    output: String,

    /// Directory of the version's spec tables (e.g. `csv/ocpp2.1`), holding
    /// the value sets the JSON schemas leave as bare strings -- standardized
    /// component and variable names, security events, reason codes, units.
    /// When given, these generate into a `standard` module alongside the
    /// message types; when omitted, no such module is written. Only
    /// meaningful when `input` is a directory.
    #[arg(long = "csv", value_name = "DIR")]
    csv: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let input_path = Path::new(&args.input);
    let version = infer_version(input_path)?;

    if input_path.is_dir() {
        generate_directory(
            input_path,
            Path::new(&args.output),
            version,
            args.csv.as_deref().map(Path::new),
        )
    } else {
        generate_single_file(input_path, Path::new(&args.output), version)
    }
}

/// Wrapper primitives (e.g. `IdTag`) are version-specific, so every
/// generation entry point needs to know which OCPP version it's targeting.
/// Rather than take it as a separate flag that could drift from the actual
/// input, it's inferred from the schema directory name -- `input` itself
/// when it's a directory, or its parent directory when it's a single file.
fn infer_version(input: &Path) -> anyhow::Result<OcppVersion> {
    let dir_name = if input.is_dir() {
        input.file_name()
    } else {
        input.parent().and_then(Path::file_name)
    }
    .and_then(|name| name.to_str())
    .ok_or_else(|| anyhow::anyhow!("could not determine a schema directory for {input:?}"))?;

    match dir_name {
        "ocpp1.6j" => Ok(OcppVersion::V16),
        "ocpp2.0.1" => Ok(OcppVersion::V201),
        "ocpp2.1" => Ok(OcppVersion::V21),
        other => anyhow::bail!(
            "unrecognized schema directory `{other}`; expected one of ocpp1.6j, ocpp2.0.1, ocpp2.1"
        ),
    }
}

fn generate_single_file(input: &Path, output: &Path, version: OcppVersion) -> anyhow::Result<()> {
    let content = fs::read_to_string(input)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;
    let model = crate::schema::SchemaParser::parse(&json, version)?;
    let code = crate::rust::generate(model);

    fs::write(output, code)?;

    Ok(())
}

/// Generates every `*.json` schema in `input_dir` into `output_dir`,
/// sharing one `TypePool` across them so a definition referenced by
/// multiple messages is written once to `common.rs` instead of once per
/// message file.
fn generate_directory(
    input_dir: &Path,
    output_dir: &Path,
    version: OcppVersion,
    csv_dir: Option<&Path>,
) -> anyhow::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(input_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    entries.sort();

    let mut schemas = Vec::with_capacity(entries.len());
    for path in &entries {
        let content = fs::read_to_string(path)?;
        schemas.push(serde_json::from_str(&content)?);
    }

    let output = crate::batch::generate_batch(&schemas, version)?;

    fs::create_dir_all(output_dir)?;
    fs::write(output_dir.join("common.rs"), &output.common)?;

    // `primitives.rs` and `error.rs` are hand-written, not generated (see
    // crates/ocpp-types/src/v16/primitives.rs) -- every version directory
    // has both, even if currently empty, so these module declarations
    // don't need to vary by version. `primitives.rs` genuinely is empty for
    // some versions (no version-specific wrapper types yet), hence the
    // `allow` -- unlike `error.rs`, which always has content.
    let mut mod_rs = format!(
        "{}pub mod common;\nmod primitives;\n#[allow(unused_imports)]\npub use primitives::*;\nmod error;\npub use error::*;\n",
        crate::rust::GENERATED_FILE_BANNER
    );

    // Declared as a module rather than glob-re-exported: several value sets
    // share a name with a schema-generated type (`UnitOfMeasure`,
    // `Component`, `IdToken`), and qualifying them as
    // `standard::ComponentName` keeps the two kinds distinguishable at the
    // use site instead of colliding in the version module's namespace.
    if let Some(csv_dir) = csv_dir
        && generate_standard_module(csv_dir, output_dir, version)?
    {
        mod_rs.push_str("pub mod standard;\n");
    }

    for message in &output.messages {
        let file_stem = crate::naming::rust_name(&message.struct_name);
        fs::write(output_dir.join(format!("{file_stem}.rs")), &message.source)?;
        mod_rs.push_str(&format!("mod {file_stem};\npub use {file_stem}::*;\n"));
    }

    fs::write(output_dir.join("mod.rs"), mod_rs)?;

    Ok(())
}

/// Generates `standard.rs` for `version` from the spec tables in `csv_dir`,
/// returning whether anything was written -- a version with no tables yet
/// gets no module rather than an empty one.
///
/// Unreadable or unparseable tables are a hard error, not a skip: silently
/// omitting a value set would leave the generated crate looking complete
/// while missing names downstream code needs.
fn generate_standard_module(
    csv_dir: &Path,
    output_dir: &Path,
    version: OcppVersion,
) -> anyhow::Result<bool> {
    if !csv_dir.is_dir() {
        anyhow::bail!("csv directory {csv_dir:?} does not exist");
    }

    let mut read_error = None;
    let mut sets = Vec::new();

    for spec in crate::standard::specs_for(version) {
        let resolved = crate::standard::resolve(spec, |file| {
            let path = csv_dir.join(file);

            if !path.is_file() {
                return None;
            }

            match fs::read_to_string(&path) {
                Ok(content) => Some(content),
                Err(error) => {
                    read_error = Some(anyhow::anyhow!("reading {path:?}: {error}"));
                    None
                }
            }
        });

        if let Some(error) = read_error.take() {
            return Err(error);
        }

        if let Some(set) = resolved {
            sets.push(set);
        }
    }

    let device_model_path = csv_dir.join("dm_components_vars.csv");
    let device_model = if device_model_path.is_file() {
        crate::standard::resolve_device_model(&fs::read_to_string(&device_model_path)?)
    } else {
        Vec::new()
    };

    // The device-model table names components and variables the dedicated
    // tables leave out, so fold those in -- but say which, since a name
    // appearing *only* there is either a real addition or a typo in the
    // export, and telling those apart needs a human.
    for set in &mut sets {
        let source = match set.name.as_str() {
            "ComponentName" => crate::standard::DeviceModelNames::Component,
            "VariableName" => crate::standard::DeviceModelNames::Variable,
            _ => continue,
        };

        let added = crate::standard::merge_device_model_names(set, &device_model, source);

        if !added.is_empty() {
            eprintln!(
                "note: {} gained {} name(s) present only in the device model table: {}",
                set.name,
                added.len(),
                added.join(", ")
            );
        }
    }

    if sets.is_empty() && device_model.is_empty() {
        return Ok(false);
    }

    fs::create_dir_all(output_dir)?;
    fs::write(
        output_dir.join("standard.rs"),
        crate::standard::generate_module(&sets, &device_model),
    )?;

    Ok(true)
}
