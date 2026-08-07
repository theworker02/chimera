//! Wire protocol version negotiation (Phase 7).

use serde::{Deserialize, Serialize};

/// Postcard / mesh wire major.minor. Bump major on breaking changes.
pub const WIRE_MAJOR: u16 = 1;
pub const WIRE_MINOR: u16 = 1;
/// Oldest major we still speak.
pub const WIRE_MIN_MAJOR: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
    pub min_major: u16,
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self {
            major: WIRE_MAJOR,
            minor: WIRE_MINOR,
            min_major: WIRE_MIN_MAJOR,
        }
    }
}

impl ProtocolVersion {
    pub fn local() -> Self {
        Self::default()
    }

    /// Returns Ok(selected) if peers can interoperate.
    pub fn negotiate(self, peer: ProtocolVersion) -> Result<ProtocolVersion, VersionError> {
        if self.major < peer.min_major || peer.major < self.min_major {
            return Err(VersionError::Incompatible {
                local: self,
                peer,
            });
        }
        // Prefer lower major if both support it (rolling upgrade: new node speaks old).
        let major = self.major.min(peer.major);
        let minor = if major == self.major && major == peer.major {
            self.minor.min(peer.minor)
        } else if major == self.major {
            self.minor
        } else {
            peer.minor
        };
        Ok(ProtocolVersion {
            major,
            minor,
            min_major: self.min_major.max(peer.min_major),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    Incompatible {
        local: ProtocolVersion,
        peer: ProtocolVersion,
    },
}

impl std::fmt::Display for VersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionError::Incompatible { local, peer } => write!(
                f,
                "incompatible wire versions local={}.{} peer={}.{}",
                local.major, local.minor, peer.major, peer.minor
            ),
        }
    }
}

impl std::error::Error for VersionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiate_same() {
        let v = ProtocolVersion::local();
        assert_eq!(v.negotiate(v).unwrap().major, 1);
    }

    #[test]
    fn reject_too_old() {
        let local = ProtocolVersion {
            major: 2,
            minor: 0,
            min_major: 2,
        };
        let peer = ProtocolVersion {
            major: 1,
            minor: 0,
            min_major: 1,
        };
        assert!(local.negotiate(peer).is_err());
    }
}
