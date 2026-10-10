use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use wowmodelexport_converter::character::CharacterAssembler;
use wowmodelexport_converter::config::ExportConfig;
use wowmodelexport_converter::export::GlbExporter;
use wowmodelexport_converter::snapshots::{ImportSnapshotsOptions, import_snapshots, list_snapshots};
use wowmodelexport_converter::wow::DataPipeline;

#[derive(Parser)]
#[command(author, version, about = "WoW 1.12 character exporter for Blender")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Validate {
        #[arg(short, long)]
        config: PathBuf,
    },
    Export {
        #[arg(short, long)]
        config: PathBuf,
    },
    SmokeTest {
        #[arg(short, long)]
        config: PathBuf,
    },
    ImportSnapshots {
        #[arg(long)]
        saved_variables: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        wow_data_path: PathBuf,
        #[arg(long)]
        output_glb_dir: PathBuf,
        #[arg(long)]
        snapshot_id: Option<usize>,
        #[arg(long)]
        list: bool,
        #[arg(long)]
        use_item_ids_as_display_ids: bool,
        #[arg(long)]
        item_display_lookup: Option<PathBuf>,
        #[arg(long)]
        db_export_lookup: Option<PathBuf>,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!(
            "{}",
            json!({
                "ok": false,
                "error": err.to_string(),
            })
        );
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Validate { config } => validate_cmd(&config),
        Commands::Export { config } => export_cmd(&config),
        Commands::SmokeTest { config } => smoke_test_cmd(&config),
        Commands::ImportSnapshots {
            saved_variables,
            output_dir,
            wow_data_path,
            output_glb_dir,
            snapshot_id,
            list,
            use_item_ids_as_display_ids,
            item_display_lookup,
            db_export_lookup,
        } => import_snapshots_cmd(
            &saved_variables,
            &output_dir,
            &wow_data_path,
            &output_glb_dir,
            snapshot_id,
            list,
            use_item_ids_as_display_ids,
            item_display_lookup.as_ref(),
            db_export_lookup.as_ref(),
        ),
    }
}

fn validate_cmd(config_path: &PathBuf) -> Result<()> {
    let cfg = ExportConfig::from_json_file(config_path)?;
    let pipeline = DataPipeline::for_config(&cfg);
    let build = pipeline.validate_client()?;
    let summary = pipeline.summarize_data()?;
    println!(
        "{}",
        json!({
            "ok": true,
            "command": "validate",
            "build": build,
            "summary": summary,
        })
    );
    Ok(())
}

fn export_cmd(config_path: &PathBuf) -> Result<()> {
    let cfg = ExportConfig::from_json_file(config_path)?;
    let pipeline = DataPipeline::for_config(&cfg);
    let build = pipeline.validate_client()?;
    let source = pipeline.load_character_source(&cfg)?;
    let assembled = CharacterAssembler::assemble(source)?;
    GlbExporter::export(&assembled, &cfg.output_path)?;
    let size = fs::metadata(&cfg.output_path)
        .with_context(|| format!("failed to inspect {}", cfg.output_path.display()))?
        .len();
    println!(
        "{}",
        json!({
            "ok": true,
            "command": "export",
            "build": build,
            "output_path": cfg.output_path,
            "output_size_bytes": size,
            "geoset_count": assembled.geosets.len(),
            "animation_count": assembled.animations.len(),
        })
    );
    Ok(())
}

fn smoke_test_cmd(config_path: &PathBuf) -> Result<()> {
    export_cmd(config_path)?;
    let cfg = ExportConfig::from_json_file(config_path)?;
    let bytes = fs::read(&cfg.output_path)
        .with_context(|| format!("failed to read {}", cfg.output_path.display()))?;
    let valid_glb = bytes.len() > 12 && bytes[0..4] == [0x67, 0x6C, 0x54, 0x46];
    println!(
        "{}",
        json!({
            "ok": valid_glb,
            "command": "smoke-test",
            "output_path": cfg.output_path,
            "detected_magic": if bytes.len() >= 4 { &bytes[0..4] } else { &[] },
            "output_size_bytes": bytes.len(),
        })
    );
    if !valid_glb {
        anyhow::bail!("smoke test failed: invalid glb header");
    }
    Ok(())
}

fn import_snapshots_cmd(
    saved_variables: &PathBuf,
    output_dir: &PathBuf,
    wow_data_path: &PathBuf,
    output_glb_dir: &PathBuf,
    snapshot_id: Option<usize>,
    list_only: bool,
    use_item_ids_as_display_ids: bool,
    item_display_lookup: Option<&PathBuf>,
    db_export_lookup: Option<&PathBuf>,
) -> Result<()> {
    if list_only {
        let snapshots = list_snapshots(saved_variables)?;
        println!(
            "{}",
            json!({
                "ok": true,
                "command": "import-snapshots",
                "mode": "list",
                "found": snapshots.len(),
                "snapshots": snapshots,
            })
        );
        return Ok(());
    }

    let opts = ImportSnapshotsOptions {
        saved_variables_path: saved_variables.clone(),
        output_dir: output_dir.clone(),
        wow_data_path: wow_data_path.clone(),
        output_glb_dir: output_glb_dir.clone(),
        snapshot_id,
        use_item_ids_as_display_ids,
        item_display_lookup_path: item_display_lookup.cloned(),
        db_export_lookup_path: db_export_lookup.cloned(),
    };

    let result = import_snapshots(&opts)?;
    println!(
        "{}",
        json!({
            "ok": true,
            "command": "import-snapshots",
            "mode": "generate",
            "found": result.found,
            "generated": result.generated,
            "resolved_slots": result.resolved_slots,
            "unresolved_slots": result.unresolved_slots,
        })
    );
    Ok(())
}
