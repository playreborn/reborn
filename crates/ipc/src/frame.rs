//! Newline-delimited JSON frames.

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// Longest accepted line. Control messages are small; this stops a broken
/// or hostile peer from making the reader buffer without limit.
pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

/// Reading or writing a frame failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FrameError {
    /// The connection failed.
    #[error("connection: {0}")]
    Io(#[from] std::io::Error),
    /// A line is not valid JSON for the expected message.
    #[error("malformed message: {0}")]
    Json(#[from] serde_json::Error),
    /// A line exceeds [`MAX_FRAME_LEN`].
    #[error("message longer than {} bytes", MAX_FRAME_LEN)]
    TooLong,
}

/// Reads one frame. Returns `None` when the peer closed the connection.
pub async fn read_frame<T: DeserializeOwned>(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> Result<Option<T>, FrameError> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err(FrameError::Io(std::io::ErrorKind::UnexpectedEof.into()))
            };
        }
        let (taken, done) = match available.iter().position(|&byte| byte == b'\n') {
            Some(end) => (end + 1, true),
            None => (available.len(), false),
        };
        line.extend_from_slice(&available[..taken]);
        reader.consume(taken);
        if line.len() > MAX_FRAME_LEN {
            return Err(FrameError::TooLong);
        }
        if done {
            return Ok(Some(serde_json::from_slice(&line)?));
        }
    }
}

/// Writes one frame followed by a newline and flushes.
pub async fn write_frame<T: Serialize>(
    writer: &mut (impl AsyncWrite + Unpin),
    frame: &T,
) -> Result<(), FrameError> {
    let mut line = serde_json::to_vec(frame)?;
    line.push(b'\n');
    writer.write_all(&line).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ClientFrame, Request};
    use tokio::io::BufReader;

    #[tokio::test]
    async fn round_trips_several_frames() {
        let mut buffer = Vec::new();
        for id in 1..=3 {
            write_frame(
                &mut buffer,
                &ClientFrame {
                    id,
                    request: Request::Status,
                },
            )
            .await
            .unwrap();
        }
        let mut reader = BufReader::new(buffer.as_slice());
        for id in 1..=3 {
            let frame: ClientFrame = read_frame(&mut reader).await.unwrap().unwrap();
            assert_eq!(frame.id, id);
        }
        assert!(
            read_frame::<ClientFrame>(&mut reader)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn rejects_garbage_and_truncated_frames() {
        let mut reader = BufReader::new(&b"not json\n"[..]);
        assert!(matches!(
            read_frame::<ClientFrame>(&mut reader).await,
            Err(FrameError::Json(_))
        ));
        let mut reader = BufReader::new(&br#"{"id":1"#[..]);
        assert!(matches!(
            read_frame::<ClientFrame>(&mut reader).await,
            Err(FrameError::Io(_))
        ));
    }
}
