use crate::client::builder::ClientBuilder;
use crate::error::Error;
use crate::types::{
    CallToolResult, ClientCapabilities, ListToolsResult, MessageContent, ReadResourceResult,
    ServerCapabilities, Tool,
};
use tokio;

async fn create_test_client() -> Result<crate::client::Client, Error> {
    ClientBuilder::new("uvx")
        .arg("notes-simple")
        .spawn_and_initialize()
        .await
}

/// Basic test verifying server capabilities
#[tokio::test]
async fn test_notes_simple_basic_functionality() -> Result<(), Error> {
    let client = create_test_client().await?;
    let caps: Option<ServerCapabilities> = client.capabilities().await;
    assert!(
        caps.is_some(),
        "Server should return capabilities after initialization"
    );
    Ok(())
}

/// Test listing tools and verifying input schema
#[tokio::test]
async fn test_list_tools_schema() -> Result<(), Error> {
    let client = create_test_client().await?;
    let tools_result = client.list_tools().await?;
    assert!(
        !tools_result.tools.is_empty(),
        "Expected at least one tool from the server"
    );

    let maybe_add_note_tool = tools_result.tools.iter().find(|t| t.name == "add-note");
    assert!(
        maybe_add_note_tool.is_some(),
        "Expected the 'add-note' tool to be listed"
    );

    let add_note_tool = maybe_add_note_tool.unwrap();
    assert_eq!(
        add_note_tool.description, "Add a new note",
        "Tool 'add-note' should have the correct description"
    );

    let schema = &add_note_tool
        .input_schema
        .as_object()
        .expect("Expected inputSchema for add-note");
    assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
    assert!(
        schema.get("properties").is_some(),
        "Expected 'properties' in inputSchema"
    );

    let required = schema
        .get("required")
        .and_then(|r| r.as_array())
        .expect("Expected 'required' array");
    let required_fields: Vec<String> = required
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    assert!(
        required_fields.contains(&"name".to_string())
            && required_fields.contains(&"content".to_string()),
        "Expected 'name' and 'content' to be required fields"
    );

    Ok(())
}

/// Calling 'add-note' successfully
#[tokio::test]
async fn test_call_add_note_success() -> Result<(), Error> {
    let client = create_test_client().await?;
    let arguments = serde_json::json!({
        "name": "my-test-note",
        "content": "This is a test note"
    });

    let call_result = client.call_tool("add-note", arguments).await?;
    // Because the server says isError=false for success
    assert_eq!(call_result.is_error, false);
    assert!(
        !call_result.content.is_empty(),
        "Expected text content after calling add-note"
    );

    if let Some(MessageContent::Text { text }) = call_result.content.first() {
        assert!(
            text.contains("my-test-note"),
            "Response text should mention the newly added note name"
        );
    }

    Ok(())
}

/// Test calling the 'add-note' tool with missing arguments -> isError=true, but not a Rust error
#[tokio::test]
async fn test_call_add_note_missing_args() -> Result<(), Error> {
    let client = create_test_client().await?;
    let arguments = serde_json::json!({ "name": "only-name-provided" });

    let call_result = client.call_tool("add-note", arguments).await?;
    assert_eq!(
        call_result.is_error, true,
        "Expected is_error=true for missing required field"
    );

    if let Some(MessageContent::Text { text }) = call_result.content.first() {
        assert!(
            text.contains("Missing name or content") 
             || text.contains("Error") // fallback
        );
    }

    Ok(())
}

/// Test calling the 'add-note' tool with invalid argument types -> isError=true
#[tokio::test]
async fn test_call_add_note_wrong_types() -> Result<(), Error> {
    let client = create_test_client().await?;
    let arguments = serde_json::json!({
        "name": "numeric-content",
        "content": 123
    });

    let call_result = client.call_tool("add-note", arguments).await?;
    assert_eq!(
        call_result.is_error, true,
        "Expected is_error=true for numeric content field"
    );

    Ok(())
}

/// Test resource listing after adding a note
#[tokio::test]
async fn test_resource_list_after_adding_note() -> Result<(), Error> {
    let client = create_test_client().await?;
    let arguments = serde_json::json!({
        "name": "listed-note",
        "content": "Note content"
    });
    let _ = client.call_tool("add-note", arguments).await?;

    let resources_value = client.request("resources/list", None).await?;
    let resources_array = resources_value
        .get("resources")
        .and_then(|val| val.as_array())
        .expect("Expected 'resources' array from server");

    let found_note = resources_array.iter().any(|res| {
        res.get("name")
            .and_then(|val| val.as_str())
            .map(|name| name.contains("listed-note"))
            .unwrap_or(false)
    });
    assert!(found_note, "Expected 'listed-note' among the resources");
    Ok(())
}

/// Test reading the content of a note that was just created
#[tokio::test]
async fn test_read_resource_of_added_note() -> Result<(), Error> {
    let client = create_test_client().await?;
    let note_name = "readable-note";
    let content_str = "Hello, I am a readable note";
    let arguments = serde_json::json!({
        "name": note_name,
        "content": content_str
    });
    client.call_tool("add-note", arguments).await?;

    let resource_uri = format!("note://internal/{}", note_name);
    let read_result = client.read_resource(&resource_uri).await?;
    assert!(
        !read_result.contents.is_empty(),
        "Expected at least one resource content block"
    );

    match &read_result.contents[0] {
        crate::types::ResourceContents::Text { text, .. } => {
            assert_eq!(text, content_str);
        }
        _ => panic!("Expected text resource content"),
    };

    Ok(())
}

/// Test calling a non-existent tool -> isError=true returned from server
#[tokio::test]
async fn test_call_tool_invalid_name() -> Result<(), Error> {
    let client = create_test_client().await?;
    let call_result = client
        .call_tool("this_tool_does_not_exist", serde_json::json!({}))
        .await?;
    assert_eq!(
        call_result.is_error, true,
        "Expected is_error=true for unknown tool"
    );
    Ok(())
}

/// Test that notifications do not disrupt subsequent requests
#[tokio::test]
async fn test_resource_list_changed_notification_handling() -> Result<(), Error> {
    let client = create_test_client().await?;

    let arguments = serde_json::json!({
        "name": "note-with-notification",
        "content": "Triggering list_changed"
    });
    client.call_tool("add-note", arguments).await?;

    let tools_result = client.list_tools().await?;
    assert!(
        !tools_result.tools.is_empty(),
        "Expected the client to still be able to list tools"
    );
    Ok(())
}

/// Additional test for ping
#[tokio::test]
async fn test_ping_request() -> Result<(), Error> {
    let client = create_test_client().await?;
    let ping_result = client.request("ping", None).await;
    match ping_result {
        Ok(val) => {
            // If the server implements ping, we expect an empty object
            if let Some(obj) = val.as_object() {
                assert!(obj.is_empty(), "Expected an empty result for ping");
            }
        }
        Err(e) => {
            // If the server doesn't implement ping, this is fine
            tracing::warn!("Ping not implemented or error: {:?}", e);
        }
    }
    Ok(())
}

/// Additional test for logging
#[tokio::test]
async fn test_set_log_level() -> Result<(), Error> {
    let client = create_test_client().await?;
    let set_result = client.request(
        "logging/setLevel",
        Some(serde_json::json!({ "level": "info" })),
    ).await;
    match set_result {
        Ok(_) => {
            tracing::info!("Successfully set log level to info");
        }
        Err(e) => {
            tracing::warn!("logging/setLevel not supported by server: {:?}", e);
        }
    }
    Ok(())
}