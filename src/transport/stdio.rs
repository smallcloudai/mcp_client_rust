use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::broadcast,
};

use crate::{
    error::{Error, ErrorCode},
    transport::{Message, Transport},
};

/// A transport that uses provided async read/write streams for MCP communication.
pub struct StdioTransport<W> {
    /// A mutex-protected writer for sending messages.
    writer: tokio::sync::Mutex<W>,
    /// A broadcast receiver for incoming messages read from the stream.
    receiver: broadcast::Receiver<Result<Message, Error>>,
    // Keep sender in scope to avoid dropping.
    _sender: broadcast::Sender<Result<Message, Error>>,
}

impl<W> StdioTransport<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    /// Creates a new StdioTransport by providing a read and a write stream.
    ///
    /// # Errors
    ///
    /// Returns an `Error` if transport creation fails (e.g., due to I/O issues).
    pub fn with_streams<R>(read: R, write: W) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let (sender, receiver) = broadcast::channel(100);
        let writer = tokio::sync::Mutex::new(write);

        let sender_clone = sender.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(read);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        tracing::debug!(
                            "EOF reached, send an EOF error so the stream ends gracefully"
                        );
                        let _ = sender_clone.send(Err(Error::Other("EOF".to_string())));
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim_end();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let message = match serde_json::from_str::<Message>(trimmed) {
                            Ok(m) => Ok(m),
                            Err(err) => Err(Error::Serialization(err.to_string())),
                        };

                        let _ = sender_clone.send(message);
                    }
                    Err(err) => {
                        let _ = sender_clone.send(Err(Error::Io(err.to_string())));
                        break;
                    }
                }
            }
        });

        Ok(StdioTransport {
            writer,
            receiver,
            _sender: sender,
        })
    }
}

#[async_trait]
impl<W> Transport for StdioTransport<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    /// Sends a message by writing JSON to the underlying writer stream,
    /// followed by a newline, and then flushing.
    async fn send(&self, message: Message) -> Result<(), Error> {
        let json = serde_json::to_string(&message)?;
        let mut writer = self.writer.lock().await;
        writer
            .write_all(json.as_bytes())
            .await
            .map_err(|e| Error::Io(e.to_string()))?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|e| Error::Io(e.to_string()))?;
        writer.flush().await.map_err(|e| Error::Io(e.to_string()))?;
        Ok(())
    }

    /// Provides a stream of incoming messages read from the stdin or other input stream.
    fn receive(&self) -> Pin<Box<dyn Stream<Item = Result<Message, Error>> + Send>> {
        let rx = self.receiver.resubscribe();
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            match rx.recv().await {
                Ok(msg) => Some((msg, rx)),
                Err(_) => None,
            }
        }))
    }

    async fn close(&self) -> Result<(), Error> {
        // No special cleanup required
        Ok(())
    }
}
