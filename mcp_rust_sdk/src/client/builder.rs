use crate::client::Client;
use crate::error::Error;
use crate::transport::stdio::StdioTransport;
use crate::types::{ClientCapabilities, Implementation};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;

pub struct ClientBuilder {
    command: String,
    args: Vec<String>,
    working_directory: Option<PathBuf>,
    implementation: Option<Implementation>,
    capabilities: Option<ClientCapabilities>,
    env: HashMap<String, String>,
}

impl ClientBuilder {
    pub fn new(command: &str) -> Self {
        Self {
            command: command.to_string(),
            args: vec![],
            working_directory: None,
            implementation: None,
            capabilities: None,
            env: HashMap::new(),
        }
    }

    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for a in args {
            self.args.push(a.as_ref().to_string());
        }
        self
    }

    pub fn directory<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.working_directory = Some(dir.into());
        self
    }

    pub fn implementation(mut self, name: &str, version: &str) -> Self {
        self.implementation = Some(Implementation {
            name: name.to_string(),
            version: version.to_string(),
        });
        self
    }

    pub fn capabilities(mut self, caps: ClientCapabilities) -> Self {
        self.capabilities = Some(caps);
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    pub async fn spawn_and_initialize(self) -> Result<Client, Error> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args);

        if let Some(dir) = &self.working_directory {
            cmd.current_dir(dir);
        }

        for (key, value) in &self.env {
            cmd.env(key, value);
        }

        cmd.stdin(Stdio::piped()).stdout(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| Error::Io(e.to_string()))?;

        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Io("No stdout".into()))?;
        let child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Io("No stdin".into()))?;

        let transport = StdioTransport::with_streams(child_stdout, child_stdin)?;
        let client = Client::new(Arc::new(transport));

        let implementation = self.implementation.unwrap_or(Implementation {
            name: "mcp-client".to_string(),
            version: "0.1.0".to_string(),
        });
        let capabilities = self.capabilities.unwrap_or_default();

        client.initialize(implementation, capabilities).await?;

        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_builder_spawn_failure() {
        // Test that spawning a non-existent command returns an error
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let builder = ClientBuilder::new("non_existent_command");
            let result = builder.spawn_and_initialize().await;
            assert!(
                result.is_err(),
                "Expected error when spawning non-existent command"
            );
        });
    }
}
 