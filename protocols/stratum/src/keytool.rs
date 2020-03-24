// Copyright (C) 2019  Braiins Systems s.r.o.
//
// This file is part of Braiins Open-Source Initiative (BOSI).
//
// BOSI is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// Please, keep in mind that we may also license BOSI or any part thereof
// under a proprietary license. For more information on the terms and conditions
// of such proprietary license or if you have any other questions, please
// contact us at opensource@braiins.com.

//! Keytool that allows:
//! - generating public/secret keypair for ED25519 curve
//! - generating and signing a stratum server certificate with a specified master secret key
//! - validating a specified certificate

use anyhow::{anyhow, Context, Result};
use ii_stratum::v2::noise;
use rand::rngs::OsRng;
use std::convert::{TryFrom, TryInto};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;
use structopt::StructOpt;

/// All commands recognized by the keytool
#[derive(Debug, StructOpt)]
#[structopt(
    name = "ii-stratum-keytool",
    about = "Tool for generating ED25519 keypairs and certificates for Stratum V2 mining protocol"
)]
enum Command {
    /// Generate CA keypair
    GenCAKey(GenCAKeyCommand),
    /// Generate Noise handshake keypair
    GenNoiseKey(GenNoiseKeyCommand),
    /// Sign a specified key
    SignKey(SignKeyCommand),
}

/// Generates keypair suitable for certification authority and stores secret and public key into
/// separate files
#[derive(Debug, StructOpt)]
struct GenCAKeyCommand {
    #[structopt(
        short = "p",
        long,
        parse(from_os_str),
        default_value = "ca-ed25519-public.key"
    )]
    public_key_file: PathBuf,
    #[structopt(
        short = "s",
        long,
        parse(from_os_str),
        default_value = "ca-ed25519-secret.key"
    )]
    secret_key_file: PathBuf,
}

impl GenCAKeyCommand {
    fn execute(self) -> Result<()> {
        print!("Generating ED25519 keypair...");

        let mut csprng = OsRng {};
        let keypair = ed25519_dalek::Keypair::generate(&mut csprng);

        write_to_file(
            &self.public_key_file,
            noise::auth::Ed25519PublicKeyFormat::new(keypair.public),
            "public key",
        )?;
        write_to_file(
            &self.secret_key_file,
            noise::auth::Ed25519SecretKeyFormat::new(keypair.secret),
            "secret key",
        )?;
        println!("DONE");

        Ok(())
    }
}

/// Generates keypair suitable for use as 's' token (static key) in noise handshake.
/// The output is stored into specified files.
#[derive(Debug, StructOpt)]
struct GenNoiseKeyCommand {
    #[structopt(
        short = "p",
        long,
        parse(from_os_str),
        default_value = "server-noise-static-public.key"
    )]
    public_key_file: PathBuf,
    #[structopt(
        short = "s",
        long,
        parse(from_os_str),
        default_value = "server-noise-static-secret.key"
    )]
    secret_key_file: PathBuf,
}

impl GenNoiseKeyCommand {
    fn execute(self) -> Result<()> {
        print!("Generating static ('s') keypair for Noise handshake ...");

        let keypair = noise::generate_keypair()
            .map_err(|e| anyhow!("Cannot generate noise keypair {}", e))?;

        write_to_file(
            &self.public_key_file,
            noise::auth::StaticPublicKeyFormat::new(keypair.public),
            "noise static public key",
        )?;
        write_to_file(
            &self.secret_key_file,
            noise::auth::StaticSecretKeyFormat::new(keypair.private),
            "noise static secret key",
        )?;
        println!("DONE");

        Ok(())
    }
}

/// Command that creates a signed certificate from a specified `public_key_to_sign`, signing the
/// certificate with `signing_key`.
#[derive(Debug, StructOpt)]
struct SignKeyCommand {
    /// File that contains the public key that we want to sign
    #[structopt(short, long, parse(from_os_str))]
    public_key_to_sign: PathBuf,
    /// Actual signing key
    #[structopt(short, long, parse(from_os_str))]
    signing_key: PathBuf,
    /// How many days the generated certificate should be valid for
    #[structopt(short, long, default_value = "90")]
    valid_for_days: usize,
}

impl SignKeyCommand {
    fn open_file(file: &PathBuf, descr: &str) -> Result<File> {
        OpenOptions::new().read(true).open(file).context(format!(
            "cannot open {} ({:?})",
            descr,
            file.clone().into_os_string()
        ))
    }

    fn read_from_file<T: TryFrom<String>>(
        file_path_buf: &PathBuf,
        error_context_descr: &str,
    ) -> Result<T>
    where
        T: TryFrom<String>,
        <T as std::convert::TryFrom<std::string::String>>::Error: std::fmt::Display,
    {
        let mut file = Self::open_file(file_path_buf, error_context_descr)?;
        let mut file_content = String::new();
        file.read_to_string(&mut file_content).context(format!(
            "Cannot read {} ({:?})",
            error_context_descr, file_path_buf
        ))?;

        let parsed_file_content = T::try_from(file_content).map_err(|e| {
            anyhow!(
                "Cannot parse {} ({:?}) {}",
                error_context_descr,
                file_path_buf,
                e
            )
        })?;

        Ok(parsed_file_content)
    }

    fn execute(self) -> Result<()> {
        let public_key = Self::read_from_file::<noise::auth::StaticPublicKeyFormat>(
            &self.public_key_to_sign,
            "static public key to sign",
        )?;

        let signing_key = Self::read_from_file::<noise::auth::Ed25519SecretKeyFormat>(
            &self.signing_key,
            "signing key",
        )?;

        let header = noise::auth::SignedPartHeader::with_duration(Duration::from_secs(
            (self.valid_for_days * 24 * 60 * 60) as u64,
        ))
        .map_err(|e| anyhow!("{}", e))?;

        let signed_part = noise::auth::SignedPart::new(header, public_key.into_inner());

        // Dalek crate requires the full Keypair for signing
        let real_signing_key = signing_key.into_inner();
        let authority_keypair = ed25519_dalek::Keypair {
            public: (&real_signing_key).into(),
            secret: real_signing_key,
        };

        let signature = signed_part
            .sign_with(&authority_keypair)
            .map_err(|e| anyhow!("{}", e))
            .context("Signing certificate")?;

        // Final step is to compose the certificate from all components and serialize it into a file
        let certificate = noise::auth::Certificate::new(signed_part, signature);
        // Derive the certificate file name from the public key filename
        let mut cert_file = self.public_key_to_sign.clone();
        cert_file.set_extension("cert");

        write_to_file(&cert_file, certificate, "certificate")
    }
}

/// Helper that opens a new file for writing or emits an error with specified context description
/// if the file already exists. This is important to prevent overwriting already generated files.
fn open_new_file(file: &PathBuf, descr: &str) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(file)
        .context(format!(
            "cannot create {} ({:?})",
            descr,
            file.clone().into_os_string()
        ))
}

/// Helper that allows writing any String serializable type `payload` to be written into a
/// specified path
fn write_to_file<T: TryInto<String>>(
    file_path_buf: &PathBuf,
    payload: T,
    error_context_descr: &str,
) -> Result<()>
where
    T: TryInto<String>,
    <T as std::convert::TryInto<std::string::String>>::Error: std::fmt::Display,
{
    let mut file = open_new_file(file_path_buf, error_context_descr)?;

    let serialized_str: String = payload.try_into().map_err(|e| {
        anyhow!(
            "Cannot serialize {} ({:?}) {}",
            error_context_descr,
            file_path_buf,
            e
        )
    })?;

    file.write_all((serialized_str + "\n").as_bytes())?;

    Ok(())
}

fn main() -> Result<()> {
    let command = Command::from_args();

    match command {
        Command::GenCAKey(gen_key_cmd) => gen_key_cmd.execute(),
        Command::GenNoiseKey(gen_key_cmd) => gen_key_cmd.execute(),
        Command::SignKey(sign_key_cmd) => sign_key_cmd.execute(),
    }
}
