use std::io::{Read, Write};
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::net::{SocketAddr, TcpStream};
use std::num::ParseIntError;
use std::time::Duration;
use byteorder::{WriteBytesExt, ReadBytesExt, BE};
use thiserror::Error;

pub fn get_status(address: &SocketAddr, timeout: Duration) -> Result<Status, PingError> {
    let mut stream = TcpStream::connect_timeout(address, timeout)?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.write_all(&[0xFE, 0x01])?;

    let packet_id = stream.read_u8()?;

    if packet_id == 0xFF {
        let response = stream.read_utf16_string()?;
        if response.starts_with("\u{00a7}1") {
            let status: Vec<&str> = response.split('\u{0}').collect();
            Ok(Status {
                dirty: true,
                version: Some(Version {
                    protocol: status[1].parse::<i16>()?,
                    server: String::from(status[2])
                }),
                motd: String::from(status[3]),
                online: (
                    status[4].parse::<u16>()?,
                    status[5].parse::<u16>()?
                )
            })
        } else {
            let status: Vec<&str> = response.split('\u{00a7}').collect();
            Ok(Status {
                dirty: true,
                version: None,
                motd: String::from(status[0]),
                online: (
                    status[1].parse::<u16>()?,
                    status[2].parse::<u16>()?
                )
            })
        }
    } else {
        Err(PingError::UnexpectedPacketId(packet_id))
    }
}

#[derive(Debug, Clone, PartialOrd, PartialEq)]
pub struct Version {
    pub protocol: i16,
    pub server: String
}

#[derive(Debug, Clone, PartialOrd, PartialEq)]
pub struct Status {
    pub dirty: bool,
    pub version: Option<Version>,
    pub motd: String,
    pub online: (u16, u16)
}

#[derive(Error, Debug)]
pub enum PingError {
    #[error("{0}")]
    Io(#[from] IoError),
    #[error("{0}")]
    ParseInt(#[from] ParseIntError),
    #[error("Unexpected packet id: {0}")]
    UnexpectedPacketId(u8),
}

pub trait PingRead: ReadBytesExt {
    fn read_var_i32(&mut self) -> Result<i32, IoError> {
        let mut x = 0i32;

        for shift in [0u32, 7, 14, 21, 28].iter() { // (0..32).step_by(7)
            let b = self.read_u8()? as i32;
            x |= (b & 0x7F) << *shift;
            if (b & 0x80) == 0 {
                return Ok(x);
            }
        }

        // The number is too large to represent in a 32-bit value.
        Err(IoError::new(IoErrorKind::InvalidInput, "VarInt too big"))
    }

    fn read_var_i64(&mut self) -> Result<i64, IoError> {
        let mut x = 0i64;

        for shift in [0u32, 7, 14, 21, 28, 35, 42, 49, 56, 63].iter() {
            let b = self.read_u8()? as i64;
            x |= (b & 0x7F) << *shift;
            if (b & 0x80) == 0 {
                return Ok(x);
            }
        }

        // The number is too large to represent in a 64-bit value.
        Err(IoError::new(IoErrorKind::InvalidInput, "VarLong too big"))
    }

    fn read_utf16_string(&mut self) -> Result<String, IoError> {
        let len = self.read_u16::<BE>()?;
        let mut chars = Vec::<u16>::new();
        for _ in 0..len {
            chars.push(self.read_u16::<BE>()?);
        }
        Ok(String::from_utf16_lossy(&chars))
    }
}

impl<R: Read> PingRead for R {}

pub trait PingWrite: WriteBytesExt {
    fn write_var_i32(&mut self, value: i32) -> Result<(), IoError> {
        let mut temp = value as u32;
        let mut result = Vec::new();
        loop {
            if (temp & !0x7fu32) == 0 {
                result.write_u8(temp as u8)?;
                self.write_all(&result)?;
                return Ok(());
            } else {
                result.write_u8(((temp & 0x7F) | 0x80) as u8)?;
                temp >>= 7;
            }
        }
    }

    fn write_var_i64(&mut self, value: i64) -> Result<(), IoError> {
        let mut temp = value as u64;
        let mut result = Vec::new();
        loop {
            if (temp & !0x7fu64) == 0 {
                result.write_u8(temp as u8)?;
                self.write_all(&result)?;
                return Ok(());
            } else {
                result.write_u8(((temp & 0x7F) | 0x80) as u8)?;
                temp >>= 7;
            }
        }
    }

    fn write_utf16_string<S>(&mut self, value: S) -> Result<(), IoError> where S: AsRef<str> {
        let encoded = value.as_ref().encode_utf16().collect::<Vec<_>>();

        self.write_u16::<BE>(encoded.len() as u16)?;
        for c in encoded {
            self.write_u16::<BE>(c)?;
        }
        Ok(())
    }
}

impl<W: Write> PingWrite for W {}