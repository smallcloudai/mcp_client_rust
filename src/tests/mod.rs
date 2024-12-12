#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;
    use crate::Config;

    #[tokio::test]
    async fn test_load_config() {
        let tmp_dir = tempdir().unwrap();
        let config_path = tmp_dir.path().join("config.json");
        let mut file = File::create(&config_path).unwrap();
        let json = r#"
        {
            "mcpServers": {
                "server1": {
                    "command": "echo",
                    "args": ["Hello"],
                    "env": {}
                }
            }
        }
        "#;
        file.write_all(json.as_bytes()).unwrap();

        let config = Config::load_config(config_path.to_str().unwrap()).unwrap();
        assert!(config.mcp_servers.contains_key("server1"));
        assert_eq!(config.mcp_servers["server1"].command, "echo");
    }
}