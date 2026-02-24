use anyhow::Result;
use std::fs;

use super::NewArgs;
use crate::config::CliConfig;

const TEMPLATE_PLUGIN_YAML: &str = include_str!("templates/plugin.yaml");
const TEMPLATE_INIT_LUA: &str = include_str!("templates/init.lua");
const TEMPLATE_HEALTH_LUA: &str = include_str!("templates/health.lua");
const TEMPLATE_LUARC_JSON: &str = include_str!("templates/.luarc.json");
const TEMPLATE_TESTS_INIT: &str = include_str!("templates/tests/init_test.lua");

pub async fn execute(_config: CliConfig, args: NewArgs) -> Result<()> {
    let output_dir = args
        .output
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let plugin_dir = output_dir.join(&args.name);

    if plugin_dir.exists() && !args.force {
        eprintln!(
            "Error: directory '{}' already exists. Use --force to overwrite.",
            args.name
        );
        std::process::exit(1);
    }

    if plugin_dir.exists() && args.force {
        fs::remove_dir_all(&plugin_dir)?;
    }

    fs::create_dir_all(&plugin_dir)?;
    fs::create_dir_all(plugin_dir.join("tests"))?;

    let plugin_yaml = TEMPLATE_PLUGIN_YAML.replace("{{name}}", &args.name);
    let init_lua = TEMPLATE_INIT_LUA.replace("{{name}}", &args.name);
    let health_lua = TEMPLATE_HEALTH_LUA.replace("{{name}}", &args.name);
    let tests_init = TEMPLATE_TESTS_INIT.replace("{{name}}", &args.name);

    fs::write(plugin_dir.join("plugin.yaml"), plugin_yaml)?;
    fs::write(plugin_dir.join("init.lua"), init_lua)?;
    fs::write(plugin_dir.join("health.lua"), health_lua)?;
    fs::write(plugin_dir.join(".luarc.json"), TEMPLATE_LUARC_JSON)?;
    fs::write(plugin_dir.join("tests/init_test.lua"), tests_init)?;

    println!(
        "✓ Plugin '{}' created at {}",
        args.name,
        plugin_dir.display()
    );
    println!();
    println!("Next steps:");
    println!("  cd {}", args.name);
    println!("  cru plugin test .");

    Ok(())
}
