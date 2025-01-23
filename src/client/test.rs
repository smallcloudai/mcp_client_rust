use crate::client::builder::ClientBuilder;
use crate::error::Error;
use crate::types::{
    CallToolResult, ClientCapabilities, ListToolsResult, MessageContent, ReadResourceResult,
    ServerCapabilities, Tool,
};
use tokio;

/// Creates a test client by spawning the `uvx` process with the `notes-simple` argument.
async fn create_test_client() -> Result<crate::client::Client, Error> {
    ClientBuilder::new("uvx")
        .arg("notes-simple")
        .spawn_and_initialize()
        .await
}

/// Basic test verifying server capabilities after initialization.
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

/// Test listing tools and verifying the returned schema.
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

    // Check that inputSchema is an object with required: [name, content]
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

/// Tests calling the 'add-note' tool successfully.
#[tokio::test]
async fn test_call_add_note_success() -> Result<(), Error> {
    let client = create_test_client().await?;
    let arguments = serde_json::json!({
        "name": "my-test-note",
        "content": "This is a test note"
    });

    let call_result = client.call_tool("add-note", arguments).await?;
    assert_eq!(call_result.is_error, false, "Tool call should succeed");
    assert!(
        !call_result.content.is_empty(),
        "Expected some text content after calling add-note"
    );

    if let Some(MessageContent::Text { text }) = call_result.content.first() {
        assert!(
            text.contains("my-test-note"),
            "Response text should mention the newly added note name"
        );
    }

    Ok(())
}

/// Tests calling the 'add-note' tool with missing arguments to ensure it returns a *tool-level* error.
#[tokio::test]
async fn test_call_add_note_missing_args() -> Result<(), Error> {
    let client = create_test_client().await?;
    let arguments = serde_json::json!({ "name": "only-name-provided" });

    let bad_result = client.call_tool("add-note", arguments).await;
    assert!(
        bad_result.is_err(),
        "Expected error when missing required fields (tool-level isError)"
    );

    Ok(())
}

/// Tests calling the 'add-note' tool with invalid argument types (e.g. numeric 'content').
#[tokio::test]
async fn test_call_add_note_wrong_types() -> Result<(), Error> {
    let client = create_test_client().await?;
    let arguments = serde_json::json!({
        "name": "numeric-content",
        "content": 123
    });

    let bad_result = client.call_tool("add-note", arguments).await;
    assert!(
        bad_result.is_err(),
        "Expected error for a numeric content field (tool-level isError)"
    );

    Ok(())
}

/// Tests retrieving a list of resources after adding a note, ensuring the new note is discoverable.
#[tokio::test]
async fn test_resource_list_after_adding_note() -> Result<(), Error> {
    let client = create_test_client().await?;
    let arguments = serde_json::json!({
        "name": "listed-note",
        "content": "Note content"
    });
    client.call_tool("add-note", arguments).await?;

    // Use a raw request to confirm the server has it in resources
    let resources_value = client.request("resources/list", None).await?;
    let resources_array = resources_value
        .get("resources")
        .and_then(|val| val.as_array())
        .expect("Expected 'resources' array in the server's response");

    let found_note = resources_array.iter().any(|res| {
        res.get("name")
            .and_then(|val| val.as_str())
            .map(|name| name.contains("listed-note"))
            .unwrap_or(false)
    });

    assert!(
        found_note,
        "Expected 'listed-note' among the listed resources"
    );

    Ok(())
}

/// Tests reading the content of a note that was just created, verifying we parse the returned JSON properly.
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
    // Use the client's read_resource helper to parse the server's response
    let read_result = client.read_resource(&resource_uri).await?;
    assert!(
        !read_result.contents.is_empty(),
        "Expected at least one resource content block"
    );

    match &read_result.contents[0] {
        crate::types::ResourceContents::Text { text, .. } => {
            assert_eq!(text, content_str, "Read content should match the original")
        }
        _ => panic!("Expected text resource content"),
    };

    Ok(())
}

/// Tests that calling a non-existent tool returns a tool-level error, which we interpret as an error in the client.
#[tokio::test]
async fn test_call_tool_invalid_name() -> Result<(), Error> {
    let client = create_test_client().await?;
    let bad_result = client
        .call_tool("this_tool_does_not_exist", serde_json::json!({}))
        .await;
    assert!(
        bad_result.is_err(),
        "Expected error when calling a non-existent tool (tool-level isError)"
    );
    Ok(())
}

/// Tests that we can handle the list_changed notification the server might emit after adding a resource.
#[tokio::test]
async fn test_resource_list_changed_notification_handling() -> Result<(), Error> {
    let client = create_test_client().await?;

    let arguments = serde_json::json!({
        "name": "note-with-notification",
        "content": "Triggering list_changed"
    });
    let _ = client.call_tool("add-note", arguments).await?;

    // Confirm the client still works after receiving notifications
    let tools_result = client.list_tools().await?;
    assert!(
        !tools_result.tools.is_empty(),
        "Expected the client to still be able to list tools"
    );
    Ok(())
}

/// Additional test for ping requests, ensuring the server responds quickly with an empty result.
#[tokio::test]
async fn test_ping_request() -> Result<(), Error> {
    let client = create_test_client().await?;
    // The server may or may not implement ping, but let's attempt anyway:
    // If unimplemented, we might get a method-not-found error. Let's check we handle it gracefully.
    let ping_result = client.request("ping", None).await;
    match ping_result {
        Ok(val) => {
            // If we got a "result": {} => that's a success
            assert!(
                val.as_object().map_or(true, |map| map.is_empty()),
                "Expected an empty result object for ping"
            );
        }
        Err(e) => {
            // If server doesn't implement ping, we may get an error. That is acceptable as well.
            tracing::warn!("Ping not implemented by server, got error: {:?}", e);
        }
    }
    Ok(())
}

/// Additional test for logging, if the server implements it. We'll set the log level and see if it returns an OK result.
#[tokio::test]
async fn test_set_log_level() -> Result<(), Error> {
    let client = create_test_client().await?;
    // The server might not implement logging. Let's just attempt "logging/setLevel".
    let set_result = client
        .request(
            "logging/setLevel",
            Some(serde_json::json!({ "level": "info" })),
        )
        .await;
    match set_result {
        Ok(_) => {
            // If it returned a result, that's a success
            tracing::info!("Successfully set log level to info");
        }
        Err(e) => {
            // If the server doesn't implement logging or doesn't allow setLevel, that's also acceptable
            tracing::warn!("Server does not support logging/setLevel: {:?}", e);
        }
    }
    Ok(())
}
