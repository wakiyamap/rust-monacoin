// Rust Bitcoin Library
// Written in 2014 by
//     Andrew Poelstra <apoelstra@wpsoftware.net>
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! Addresses
//!
//! Support for ordinary base58 Bitcoin addresses and private keys
//!
//! # Example: creating a new address from a randomly-generated key pair
//!
//! ```rust
//!
//! use monacoin::network::constants::Network;
//! use monacoin::util::address::Address;
//! use monacoin::util::key;
//! use monacoin::secp256k1::Secp256k1;
//! use monacoin::secp256k1::rand::thread_rng;
//!
//! // Generate random key pair
//! let s = Secp256k1::new();
//! let public_key = key::PublicKey {
//!     compressed: true,
//!     key: s.generate_keypair(&mut thread_rng()).1,
//! };
//!
//! // Generate pay-to-pubkey-hash address
//! let address = Address::p2pkh(&public_key, Network::Monacoin);
//! ```

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use bech32;
use hashes::Hash;
use hash_types::{PubkeyHash, WPubkeyHash, ScriptHash, WScriptHash};
use blockdata::script;
use network::constants::Network;
use util::base58;
use util::key;

/// Address error.
#[derive(Debug, PartialEq)]
pub enum Error {
    /// Base58 encoding error
    Base58(base58::Error),
    /// Bech32 encoding error
    Bech32(bech32::Error),
    /// The bech32 payload was empty
    EmptyBech32Payload,
    /// Script version must be 0 to 16 inclusive
    InvalidWitnessVersion(u8),
    /// The witness program must be between 2 and 40 bytes in length.
    InvalidWitnessProgramLength(usize),
    /// A v0 witness program must be either of length 20 or 32.
    InvalidSegwitV0ProgramLength(usize),
    /// An uncompressed pubkey was used where it is not allowed.
    UncompressedPubkey,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Base58(ref e) => write!(f, "base58: {}", e),
            Error::Bech32(ref e) => write!(f, "bech32: {}", e),
            Error::EmptyBech32Payload => write!(f, "the bech32 payload was empty"),
            Error::InvalidWitnessVersion(v) => write!(f, "invalid witness script version: {}", v),
            Error::InvalidWitnessProgramLength(l) => write!(f,
                "the witness program must be between 2 and 40 bytes in length: length={}", l,
            ),
            Error::InvalidSegwitV0ProgramLength(l) => write!(f,
                "a v0 witness program must be either of length 20 or 32 bytes: length={}", l,
            ),
            Error::UncompressedPubkey => write!(f,
                "an uncompressed pubkey was used where it is not allowed",
            ),
        }
    }
}

#[allow(deprecated)]
impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::Base58(ref e) => Some(e),
            Error::Bech32(ref e) => Some(e),
            _ => None,
        }
    }

    fn description(&self) -> &str {
        "description() is deprecated; use Display"
    }
}

#[doc(hidden)]
impl From<base58::Error> for Error {
    fn from(e: base58::Error) -> Error {
        Error::Base58(e)
    }
}

#[doc(hidden)]
impl From<bech32::Error> for Error {
    fn from(e: bech32::Error) -> Error {
        Error::Bech32(e)
    }
}

/// The different types of addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AddressType {
    /// pay-to-pubkey-hash
    P2pkh,
    /// pay-to-script-hash
    P2sh,
    /// pay-to-witness-pubkey-hash
    P2wpkh,
    /// pay-to-witness-script-hash
    P2wsh,
}

impl fmt::Display for AddressType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match *self {
            AddressType::P2pkh => "p2pkh",
            AddressType::P2sh => "p2sh",
            AddressType::P2wpkh => "p2wpkh",
            AddressType::P2wsh => "p2wsh",
        })
    }
}

impl FromStr for AddressType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "p2pkh" => Ok(AddressType::P2pkh),
            "p2sh" => Ok(AddressType::P2sh),
            "p2wpkh" => Ok(AddressType::P2wpkh),
            "p2wsh" => Ok(AddressType::P2wsh),
            _ => Err(()),
        }
    }
}

/// The method used to produce an address
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Payload {
    /// P2PKH address
    PubkeyHash(PubkeyHash),
    /// P2SH address
    ScriptHash(ScriptHash),
    /// Segwit addresses
    WitnessProgram {
        /// The witness program version
        version: bech32::u5,
        /// The witness program
        program: Vec<u8>,
    },
}

impl Payload {
    /// Get a [Payload] from an output script (scriptPubkey).
    pub fn from_script(script: &script::Script) -> Option<Payload> {
        Some(if script.is_p2pkh() {
            Payload::PubkeyHash(PubkeyHash::from_slice(&script.as_bytes()[3..23]).unwrap())
        } else if script.is_p2sh() {
            Payload::ScriptHash(ScriptHash::from_slice(&script.as_bytes()[2..22]).unwrap())
        } else if script.is_witness_program() {
            // We can unwrap the u5 check and assume script length
            // because [Script::is_witness_program] makes sure of this.
            Payload::WitnessProgram {
                version: {
                    // Since we passed the [is_witness_program] check,
                    // the first byte is either 0x00 or 0x50 + version.
                    let mut verop = script.as_bytes()[0];
                    if verop > 0x50 {
                        verop -= 0x50;
                    }
                    bech32::u5::try_from_u8(verop).expect("checked before")
                },
                program: script.as_bytes()[2..].to_vec(),
            }
        } else {
            return None;
        })
    }

    /// Generates a script pubkey spending to this [Payload].
    pub fn script_pubkey(&self) -> script::Script {
        match *self {
            Payload::PubkeyHash(ref hash) =>
                script::Script::new_p2pkh(hash),
            Payload::ScriptHash(ref hash) =>
                script::Script::new_p2sh(hash),
            Payload::WitnessProgram {
                version: ver,
                program: ref prog,
            } => script::Script::new_witness_program(ver, prog)
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A Monacoin address
pub struct Address {
    /// The type of the address
    pub payload: Payload,
    /// The network on which this address is usable
    pub network: Network,
}
serde_string_impl!(Address, "a Monacoin address");

impl Address {
    /// Creates a pay to (compressed) public key hash address from a public key
    /// This is the preferred non-witness type address
    #[inline]
    pub fn p2pkh(pk: &key::PublicKey, network: Network) -> Address {
        let mut hash_engine = PubkeyHash::engine();
        pk.write_into(&mut hash_engine);

        Address {
            network: network,
            payload: Payload::PubkeyHash(PubkeyHash::from_engine(hash_engine)),
        }
    }

    /// Creates a pay to script hash P2SH address from a script
    /// This address type was introduced with BIP16 and is the popular type to implement multi-sig these days.
    #[inline]
    pub fn p2sh(script: &script::Script, network: Network) -> Address {
        Address {
            network: network,
            payload: Payload::ScriptHash(ScriptHash::hash(&script[..])),
        }
    }

    /// Create a witness pay to public key address from a public key
    /// This is the native segwit address type for an output redeemable with a single signature
    ///
    /// Will only return an Error when an uncompressed public key is provided.
    pub fn p2wpkh(pk: &key::PublicKey, network: Network) -> Result<Address, Error> {
        if !pk.compressed {
            return Err(Error::UncompressedPubkey);
        }

        let mut hash_engine = WPubkeyHash::engine();
        pk.write_into(&mut hash_engine);

        Ok(Address {
            network: network,
            payload: Payload::WitnessProgram {
                version: bech32::u5::try_from_u8(0).expect("0<32"),
                program: WPubkeyHash::from_engine(hash_engine)[..].to_vec(),
            },
        })
    }

    /// Create a pay to script address that embeds a witness pay to public key
    /// This is a segwit address type that looks familiar (as p2sh) to legacy clients
    ///
    /// Will only return an Error when an uncompressed public key is provided.
    pub fn p2shwpkh(pk: &key::PublicKey, network: Network) -> Result<Address, Error> {
        if !pk.compressed {
            return Err(Error::UncompressedPubkey);
        }

        let mut hash_engine = WPubkeyHash::engine();
        pk.write_into(&mut hash_engine);

        let builder = script::Builder::new()
            .push_int(0)
            .push_slice(&WPubkeyHash::from_engine(hash_engine)[..]);

        Ok(Address {
            network: network,
            payload: Payload::ScriptHash(ScriptHash::hash(builder.into_script().as_bytes())),
        })
    }

    /// Create a witness pay to script hash address
    pub fn p2wsh(script: &script::Script, network: Network) -> Address {
        Address {
            network: network,
            payload: Payload::WitnessProgram {
                version: bech32::u5::try_from_u8(0).expect("0<32"),
                program: WScriptHash::hash(&script[..])[..].to_vec(),
            },
        }
    }

    /// Create a pay to script address that embeds a witness pay to script hash address
    /// This is a segwit address type that looks familiar (as p2sh) to legacy clients
    pub fn p2shwsh(script: &script::Script, network: Network) -> Address {
        let ws = script::Builder::new()
            .push_int(0)
            .push_slice(&WScriptHash::hash(&script[..])[..])
            .into_script();

        Address {
            network: network,
            payload: Payload::ScriptHash(ScriptHash::hash(&ws[..])),
        }
    }

    /// Get the address type of the address.
    /// None if unknown or non-standard.
    pub fn address_type(&self) -> Option<AddressType> {
        match self.payload {
            Payload::PubkeyHash(_) => Some(AddressType::P2pkh),
            Payload::ScriptHash(_) => Some(AddressType::P2sh),
            Payload::WitnessProgram {
                version: ver,
                program: ref prog,
            } => {
                // BIP-141 p2wpkh or p2wsh addresses.
                match ver.to_u8() {
                    0 => match prog.len() {
                        20 => Some(AddressType::P2wpkh),
                        32 => Some(AddressType::P2wsh),
                        _ => None,
                    },
                    _ => None,
                }
            }
        }
    }

    /// Check whether or not the address is following Monacoin
    /// standardness rules.
    ///
    /// Segwit addresses with unassigned witness versions or non-standard
    /// program sizes are considered non-standard.
    pub fn is_standard(&self) -> bool {
        self.address_type().is_some()
    }

    /// Get an [Address] from an output script (scriptPubkey).
    pub fn from_script(script: &script::Script, network: Network) -> Option<Address> {
        Some(Address {
            payload: Payload::from_script(script)?,
            network: network,
        })
    }

    /// Generates a script pubkey spending to this address
    pub fn script_pubkey(&self) -> script::Script {
        self.payload.script_pubkey()
    }
}

impl Display for Address {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match self.payload {
            Payload::PubkeyHash(ref hash) => {
                let mut prefixed = [0; 21];
                prefixed[0] = match self.network {
                    Network::Monacoin => 50,
                    Network::MonacoinTestnet | Network::MonacoinRegtest => 111,
                };
                prefixed[1..].copy_from_slice(&hash[..]);
                base58::check_encode_slice_to_fmt(fmt, &prefixed[..])
            }
            Payload::ScriptHash(ref hash) => {
                let mut prefixed = [0; 21];
                prefixed[0] = match self.network {
                    Network::Monacoin => 55,
                    Network::MonacoinTestnet | Network::MonacoinRegtest => 117,
                };
                prefixed[1..].copy_from_slice(&hash[..]);
                base58::check_encode_slice_to_fmt(fmt, &prefixed[..])
            }
            Payload::WitnessProgram {
                version: ver,
                program: ref prog,
            } => {
                let hrp = match self.network {
                    Network::Monacoin => "mona",
                    Network::MonacoinTestnet => "tmona",
                    Network::MonacoinRegtest => "rmona",
                };
                let mut bech32_writer = bech32::Bech32Writer::new(hrp, fmt)?;
                bech32::WriteBase32::write_u5(&mut bech32_writer, ver)?;
                bech32::ToBase32::write_base32(&prog, &mut bech32_writer)
            }
        }
    }
}

/// Extract the bech32 prefix.
/// Returns the same slice when no prefix is found.
fn find_bech32_prefix(bech32: &str) -> &str {
    // Split at the last occurrence of the separator character '1'.
    match bech32.rfind('1') {
        None => bech32,
        Some(sep) => bech32.split_at(sep).0,
    }
}

impl FromStr for Address {
    type Err = Error;

    fn from_str(s: &str) -> Result<Address, Error> {
        // try bech32
        let bech32_network = match find_bech32_prefix(s) {
            // note that upper or lowercase is allowed but NOT mixed case
            "mona" | "MONA" => Some(Network::Monacoin),
            "tmona" | "TMONA" => Some(Network::MonacoinTestnet),
            "rmona" | "RMONA" => Some(Network::MonacoinRegtest),
            _ => None,
        };
        if let Some(network) = bech32_network {
            // decode as bech32
            let (_, payload) = bech32::decode(s)?;
            if payload.is_empty() {
                return Err(Error::EmptyBech32Payload);
            }

            // Get the script version and program (converted from 5-bit to 8-bit)
            let (version, program): (bech32::u5, Vec<u8>) = {
                let (v, p5) = payload.split_at(1);
                (v[0], bech32::FromBase32::from_base32(p5)?)
            };

            // Generic segwit checks.
            if version.to_u8() > 16 {
                return Err(Error::InvalidWitnessVersion(version.to_u8()));
            }
            if program.len() < 2 || program.len() > 40 {
                return Err(Error::InvalidWitnessProgramLength(program.len()));
            }

            // Specific segwit v0 check.
            if version.to_u8() == 0 && (program.len() != 20 && program.len() != 32) {
                return Err(Error::InvalidSegwitV0ProgramLength(program.len()));
            }

            return Ok(Address {
                payload: Payload::WitnessProgram {
                    version: version,
                    program: program,
                },
                network: network,
            });
        }

        // Base58
        if s.len() > 50 {
            return Err(Error::Base58(base58::Error::InvalidLength(s.len() * 11 / 15)));
        }
        let data = base58::from_check(s)?;
        if data.len() != 21 {
            return Err(Error::Base58(base58::Error::InvalidLength(data.len())));
        }

        let (network, payload) = match data[0] {
            50 => (
                Network::Monacoin,
                Payload::PubkeyHash(PubkeyHash::from_slice(&data[1..]).unwrap()),
            ),
            55 => (
                Network::Monacoin,
                Payload::ScriptHash(ScriptHash::from_slice(&data[1..]).unwrap()),
            ),
            111 => (
                Network::MonacoinTestnet,
                Payload::PubkeyHash(PubkeyHash::from_slice(&data[1..]).unwrap()),
            ),
            117 => (
                Network::MonacoinTestnet,
                Payload::ScriptHash(ScriptHash::from_slice(&data[1..]).unwrap()),
            ),
            x => return Err(Error::Base58(base58::Error::InvalidVersion(vec![x]))),
        };

        Ok(Address {
            network: network,
            payload: payload,
        })
    }
}

impl ::std::fmt::Debug for Address {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::string::ToString;

    use hashes::hex::{FromHex, ToHex};

    use blockdata::script::Script;
    use network::constants::Network::{Monacoin, MonacoinTestnet};
    use util::key::PublicKey;

    use super::*;

    macro_rules! hex (($hex:expr) => (Vec::from_hex($hex).unwrap()));
    macro_rules! hex_key (($hex:expr) => (PublicKey::from_slice(&hex!($hex)).unwrap()));
    macro_rules! hex_script (($hex:expr) => (Script::from(hex!($hex))));
    macro_rules! hex_pubkeyhash (($hex:expr) => (PubkeyHash::from_hex(&$hex).unwrap()));
    macro_rules! hex_scripthash (($hex:expr) => (ScriptHash::from_hex($hex).unwrap()));

    fn roundtrips(addr: &Address) {
        assert_eq!(
            Address::from_str(&addr.to_string()).unwrap(),
            *addr,
            "string round-trip failed for {}",
            addr,
        );
        assert_eq!(
            Address::from_script(&addr.script_pubkey(), addr.network).as_ref(),
            Some(addr),
            "script round-trip failed for {}",
            addr,
        );
        //TODO: add serde roundtrip after no-strason PR
    }

    #[test]
    fn test_p2pkh_address_58() {
        let addr = Address {
            network: Monacoin,
            payload: Payload::PubkeyHash(hex_pubkeyhash!("162c5ea71c0b23f5b9022ef047c4a86470a5b070")),
        };

        assert_eq!(
            addr.script_pubkey(),
            hex_script!("76a914162c5ea71c0b23f5b9022ef047c4a86470a5b07088ac")
        );
        assert_eq!(&addr.to_string(), "M9vQFWksNwMShpHKZJqDdMPFjkyGDRtxyn");
        assert_eq!(addr.address_type(), Some(AddressType::P2pkh));
        roundtrips(&addr);
    }

    #[test]
    fn test_p2pkh_from_key() {
        let key = hex_key!("048d5141948c1702e8c95f438815794b87f706a8d4cd2bffad1dc1570971032c9b6042a0431ded2478b5c9cf2d81c124a5e57347a3c63ef0e7716cf54d613ba183");
        let addr = Address::p2pkh(&key, Monacoin);
        assert_eq!(&addr.to_string(), "MXCeTRYF62fdUtHT8CJujoaVEduGf5hQP6");

        let key = hex_key!(&"03df154ebfcf29d29cc10d5c2565018bce2d9edbab267c31d2caf44a63056cf99f");
        let addr = Address::p2pkh(&key, MonacoinTestnet);
        assert_eq!(&addr.to_string(), "mqkhEMH6NCeYjFybv7pvFC22MFeaNT9AQC");
        assert_eq!(addr.address_type(), Some(AddressType::P2pkh));
        roundtrips(&addr);
    }

    #[test]
    fn test_p2sh_address_58() {
        let addr = Address {
            network: Monacoin,
            payload: Payload::ScriptHash(hex_scripthash!("162c5ea71c0b23f5b9022ef047c4a86470a5b070")),
        };

        assert_eq!(
            addr.script_pubkey(),
            hex_script!("a914162c5ea71c0b23f5b9022ef047c4a86470a5b07087")
        );
        assert_eq!(&addr.to_string(), "PAcRB4FJvqfpnyykgQVp3ykBtHFyr1o6G1");
        assert_eq!(addr.address_type(), Some(AddressType::P2sh));
        roundtrips(&addr);
    }

    #[test]
    fn test_p2sh_parse() {
        let script = hex_script!("552103a765fc35b3f210b95223846b36ef62a4e53e34e2925270c2c7906b92c9f718eb2103c327511374246759ec8d0b89fa6c6b23b33e11f92c5bc155409d86de0c79180121038cae7406af1f12f4786d820a1466eec7bc5785a1b5e4a387eca6d797753ef6db2103252bfb9dcaab0cd00353f2ac328954d791270203d66c2be8b430f115f451b8a12103e79412d42372c55dd336f2eb6eb639ef9d74a22041ba79382c74da2338fe58ad21035049459a4ebc00e876a9eef02e72a3e70202d3d1f591fc0dd542f93f642021f82102016f682920d9723c61b27f562eb530c926c00106004798b6471e8c52c60ee02057ae");
        let addr = Address::p2sh(&script, MonacoinTestnet);

        assert_eq!(&addr.to_string(), "pGHsx2PsNMnehUxHwKzxwo19yz3gmQfgqd");
        assert_eq!(addr.address_type(), Some(AddressType::P2sh));
        roundtrips(&addr);
    }

    #[test]
    fn test_p2wpkh() {
        // stolen from Bitcoin transaction: b3c8c2b6cfc335abbcb2c7823a8453f55d64b2b5125a9a61e8737230cdb8ce20
        let mut key = hex_key!("033bc8c83c52df5712229a2f72206d90192366c36428cb0c12b6af98324d97bfbc");
        let addr = Address::p2wpkh(&key, Monacoin).unwrap();
        assert_eq!(&addr.to_string(), "mona1qvzvkjn4q3nszqxrv3nraga2r822xjty3q96530");
        assert_eq!(addr.address_type(), Some(AddressType::P2wpkh));
        roundtrips(&addr);

        // Test uncompressed pubkey
        key.compressed = false;
        assert_eq!(Address::p2wpkh(&key, Monacoin), Err(Error::UncompressedPubkey));
    }

    #[test]
    fn test_p2wsh() {
        // stolen from Monacoin transaction 5df912fda4becb1c29e928bec8d64d93e9ba8efa9b5b405bd683c86fd2c65667
        let script = hex_script!("52210375e00eb72e29da82b89367947f29ef34afb75e8654f6ea368e0acdfd92976b7c2103a1b26313f430c4b15bb1fdce663207659d8cac749a0e53d70eff01874496feff2103c96d495bfdd5ba4145e3e046fee45e84a8a48ad05bd8dbb395c011a32cf9f88053ae");
        let addr = Address::p2wsh(&script, Monacoin);
        assert_eq!(
            &addr.to_string(),
            "mona1qwqdg6squsna38e46795at95yu9atm8azzmyvckulcc7kytlcckxs824ha6"
        );
        assert_eq!(addr.address_type(), Some(AddressType::P2wsh));
        roundtrips(&addr);
    }

    #[test]
    fn test_p2shwpkh() {
        // stolen from Bitcoin transaction: ad3fd9c6b52e752ba21425435ff3dd361d6ac271531fc1d2144843a9f550ad01
        let mut key = hex_key!("026c468be64d22761c30cd2f12cbc7de255d592d7904b1bab07236897cc4c2e766");
        let addr = Address::p2shwpkh(&key, Monacoin).unwrap();
        assert_eq!(&addr.to_string(), "PX5azwHFLCSaXpdBLe1gX7VuN2wrKcbcFw");
        assert_eq!(addr.address_type(), Some(AddressType::P2sh));
        roundtrips(&addr);

        // Test uncompressed pubkey
        key.compressed = false;
        assert_eq!(Address::p2wpkh(&key, Monacoin), Err(Error::UncompressedPubkey));
    }

    #[test]
    fn test_p2shwsh() {
        // stolen from Monacoin transaction f9ee2be4df05041d0e0a35d7caa3157495ca4f93b233234c9967b6901dacf7a9
        let script = hex_script!("522103e5529d8eaa3d559903adb2e881eb06c86ac2574ffa503c45f4e942e2a693b33e2102e5f10fcdcdbab211e0af6a481f5532536ec61a5fdbf7183770cf8680fe729d8152ae");
        let addr = Address::p2shwsh(&script, Monacoin);
        assert_eq!(&addr.to_string(), "PD8zuohH1XJsLXctuAAYFRyXaAhR5qD44F");
        assert_eq!(addr.address_type(), Some(AddressType::P2sh));
        roundtrips(&addr);
    }

    #[test]
    fn test_non_existent_segwit_version() {
        let version = 13;
        // 40-byte program
        let program = hex!(
            "654f6ea368e0acdfd92976b7c2103a1b26313f430654f6ea368e0acdfd92976b7c2103a1b26313f4"
        );
        let addr = Address {
            payload: Payload::WitnessProgram {
                version: bech32::u5::try_from_u8(version).expect("0<32"),
                program: program,
            },
            network: Network::Monacoin,
        };
        roundtrips(&addr);
    }

    #[test]
    fn test_bip173_vectors() {
        let valid_vectors = [
            ("MONA1Q4KPN6PSTHGD5UR894AUHJJ2G02WLGMP8KE08NE", "0014ad833d060bba1b4e0ce5af797949487a9df46c27"),
            ("mona1qp8f842ywwr9h5rdxyzggex7q3trvvvaarfssxccju52rj6htfzfsqr79j2", "002009d27aa88e70cb7a0da620908c9bc08ac6c633bd1a61036312e514396aeb4893"),
            ("mona1zw508d6qejxtdg4y5r3zarvaryvhm3vz7", "5210751e76e8199196d454941c45d1b3a323"),
            ("mona1sw50qpvnxy8", "6002751e"),
            ("tmona1qfj8lu0rafk2mpvk7jj62q8eerjpex3xlcadtupkrkhh5a73htmhs68e55m", "00204c8ffe3c7d4d95b0b2de94b4a01f391c839344dfc75abe06c3b5ef4efa375eef"),
        ];
        for vector in &valid_vectors {
            let addr: Address = vector.0.parse().unwrap();
            assert_eq!(&addr.script_pubkey().as_bytes().to_hex(), vector.1);
            roundtrips(&addr);
        }

        let invalid_vectors = [
            "tnamo1qw508d6qejxtdg4y5r3zarvary0c5xw7kumwy3n",
            "mona1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5c2see9",
            "MONA13W508D6QEJXTDG4Y5R3ZARVARY0C5XW7KHXEVYT",
            "mona1rw5gv2qqg",
            "mona10w508d6qejxtdg4y5r3zarvary0c5xw7kw508d6qejxtdg4y5r3zarvary0c5xw7kw56ms2yd",
            "MONA1QR508D6QEJXTDG4Y5R3ZARVARYV6X0N6D",
            "tmona1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qwLyd0j",
            "mona1zw508d6qejxtdg4y5r3zarvaryvq0fn2th",
            "tmona1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3pnfscjq",
            "mona1c0fp8z",
        ];
        for vector in &invalid_vectors {
            assert!(vector.parse::<Address>().is_err());
        }
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_json_serialize() {
        use serde_json;

        let addr = Address::from_str("M9vQFWksNwMShpHKZJqDdMPFjkyGDRtxyn").unwrap();
        let json = serde_json::to_value(&addr).unwrap();
        assert_eq!(
            json,
            serde_json::Value::String("M9vQFWksNwMShpHKZJqDdMPFjkyGDRtxyn".to_owned())
        );
        let into: Address = serde_json::from_value(json).unwrap();
        assert_eq!(addr.to_string(), into.to_string());
        assert_eq!(
            into.script_pubkey(),
            hex_script!("76a914162c5ea71c0b23f5b9022ef047c4a86470a5b07088ac")
        );

        let addr = Address::from_str("PAcRB4FJvqfpnyykgQVp3ykBtHFyr1o6G1").unwrap();
        let json = serde_json::to_value(&addr).unwrap();
        assert_eq!(
            json,
            serde_json::Value::String("PAcRB4FJvqfpnyykgQVp3ykBtHFyr1o6G1".to_owned())
        );
        let into: Address = serde_json::from_value(json).unwrap();
        assert_eq!(addr.to_string(), into.to_string());
        assert_eq!(
            into.script_pubkey(),
            hex_script!("a914162c5ea71c0b23f5b9022ef047c4a86470a5b07087")
        );

        let addr =
            Address::from_str("tmona1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qwlyd0j")
                .unwrap();
        let json = serde_json::to_value(&addr).unwrap();
        assert_eq!(
            json,
            serde_json::Value::String(
                "tmona1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qwlyd0j".to_owned()
            )
        );
        let into: Address = serde_json::from_value(json).unwrap();
        assert_eq!(addr.to_string(), into.to_string());
        assert_eq!(
            into.script_pubkey(),
            hex_script!("00201863143c14c5166804bd19203356da136c985678cd4d27a1b8c6329604903262")
        );

        let addr = Address::from_str("rmona1q2nfxmhd4n3c8834pj72xagvyr9gl57n5fllfhk").unwrap();
        let json = serde_json::to_value(&addr).unwrap();
        assert_eq!(
            json,
            serde_json::Value::String("rmona1q2nfxmhd4n3c8834pj72xagvyr9gl57n5fllfhk".to_owned())
        );
        let into: Address = serde_json::from_value(json).unwrap();
        assert_eq!(addr.to_string(), into.to_string());
        assert_eq!(
            into.script_pubkey(),
            hex_script!("001454d26dddb59c7073c6a197946ea1841951fa7a74")
        );
    }
}
