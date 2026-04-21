use super::super::IncludeConfig;

#[test]
fn test_include_config_empty() {
    let config = IncludeConfig::default();
    assert!(config.is_empty());
}

#[test]
fn test_include_config_with_gateway() {
    let toml_content = r#"
gateway = "mcps.toml"
"#;
    let config: IncludeConfig = toml::from_str(toml_content).unwrap();

    assert!(!config.is_empty());
    assert_eq!(config.gateway, Some("mcps.toml".to_string()));
}

#[test]
fn test_include_config_all_includes() {
    let toml_content = r#"
gateway = "mcps.toml"
discovery = "discovery.toml"
hooks = "hooks.toml"
enrichment = "enrichment.toml"
custom_section = "custom.toml"
"#;
    let config: IncludeConfig = toml::from_str(toml_content).unwrap();

    let includes = config.all_includes();
    assert_eq!(includes.len(), 5);

    let section_names: Vec<&str> = includes.iter().map(|(s, _)| *s).collect();
    assert!(section_names.contains(&"gateway"));
    assert!(section_names.contains(&"discovery"));
    assert!(section_names.contains(&"hooks"));
    assert!(section_names.contains(&"enrichment"));
    assert!(section_names.contains(&"custom_section"));
}
