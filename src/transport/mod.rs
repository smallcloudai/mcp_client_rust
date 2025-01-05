use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use std::fmt;

use crate::protocol::{Notification, Request, Response};
use crate::Error;

/// A message that can be sent over a transport
#[derive(Debug, Clone)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

impl Message {
    fn message_type(&self) -> &'static str {
        match self {
            Message::Request(_) => "request",
            Message::Response(_) => "response",
            Message::Notification(_) => "notification",
        }
    }
}

// Custom visitor for more efficient deserialization
struct MessageVisitor;

impl<'de> Visitor<'de> for MessageVisitor {
    type Value = Message;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JSON-RPC 2.0 message")
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        eprintln!("Attempting to deserialize message...");

        // Collect all fields into a Value first
        let mut obj = serde_json::Map::new();
        
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value()?;
            obj.insert(key, value);
        }

        // Now analyze the collected object
        let value = serde_json::Value::Object(obj);
        
        // Determine message type based on JSON-RPC 2.0 spec
        if let Some(id_val) = value.get("id") {
            // If `id` is present, it must be a valid string or number for request/response
            if value.get("method").is_some() {
                // Request must have `method` and `id`
                eprintln!("Deserializing as request...");
                eprintln!("Value: {:?}", value);
                Ok(Message::Request(Request::deserialize(value).map_err(de::Error::custom)?))
            } else if value.get("result").is_some() || value.get("error").is_some() {
                // Response must have `id` and either result or error
                eprintln!("Deserializing as response...");
                eprintln!("Value: {:?}", value);
                Ok(Message::Response(Response::deserialize(value).map_err(de::Error::custom)?))
            } else {
                // `id` present but no `method` or `result/error` => invalid
                Err(de::Error::custom("invalid message: 'id' present without 'method' or 'result/error'"))
            }
        } else if value.get("method").is_some() {
            // Notification (no id, has method)
            eprintln!("Deserializing as notification...");
            eprintln!("Value: {:?}", value);
            Ok(Message::Notification(Notification::deserialize(value).map_err(de::Error::custom)?))
        } else {
            // No `id`, no `method` => invalid
            Err(de::Error::custom("invalid message: missing 'id' and 'method'"))
        }
    }
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(MessageVisitor)
    }
}

impl Serialize for Message {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        
        // Always add type field for our protocol
        map.serialize_entry("type", self.message_type())?;
        
        // Add message-specific fields
        match self {
            Message::Request(req) => {
                map.serialize_entry("jsonrpc", &req.jsonrpc)?;
                map.serialize_entry("method", &req.method)?;
                if let Some(ref params) = req.params {
                    map.serialize_entry("params", params)?;
                }
                map.serialize_entry("id", &req.id)?;
            }
            Message::Response(resp) => {
                map.serialize_entry("jsonrpc", &resp.jsonrpc)?;
                map.serialize_entry("id", &resp.id)?;
                if let Some(ref result) = resp.result {
                    map.serialize_entry("result", result)?;
                }
                if let Some(ref error) = resp.error {
                    map.serialize_entry("error", error)?;
                }
            }
            Message::Notification(notif) => {
                map.serialize_entry("jsonrpc", &notif.jsonrpc)?;
                map.serialize_entry("method", &notif.method)?;
                if let Some(ref params) = notif.params {
                    map.serialize_entry("params", params)?;
                }
            }
        }
        
        map.end()
    }
}

/// Trait for implementing MCP transports
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Send a message over the transport
    async fn send(&self, message: Message) -> Result<(), Error>;

    /// Receive messages from the transport
    fn receive(&self) -> Pin<Box<dyn Stream<Item = Result<Message, Error>> + Send>>;

    /// Close the transport
    async fn close(&self) -> Result<(), Error>;
}

pub mod stdio;
