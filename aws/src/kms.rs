use crate::AwsClientConfig;
use failure::{Error, Fail};
use log::{debug, warn};
use rusoto_kms::{DecryptError, DecryptRequest, Kms, KmsClient};
use std::str;

#[derive(Debug, Fail)]
enum KmsError {
    #[fail(display = "decryption failed ")]
    DecryptionFailed,
    #[fail(display = "no plain text found after decryption")]
    NoPlainText,
}

pub fn decrypt_base64(aws_client_config: &AwsClientConfig, base64_str: &str) -> Result<String, Error> {
    do_decrypt_base64(aws_client_config, base64_str).map_err(|e| e.context(KmsError::DecryptionFailed).into())
}

fn do_decrypt_base64(aws_client_config: &AwsClientConfig, base64_str: &str) -> Result<String, Error> {
    debug!("Decrypting base64 str.");
    let blob = base64::decode(base64_str)?;

    let credentials_provider = aws_client_config.credentials_provider.clone();
    let http_client = aws_client_config.http_client.clone();
    let kms = KmsClient::new_with(http_client, credentials_provider, aws_client_config.region.clone());
    let decrypt_request = DecryptRequest {
        ciphertext_blob: blob,
        encryption_context: None,
        grant_tokens: None,
    };
    let res = kms.decrypt(decrypt_request).sync();
    debug!("Finished decrypting base64 str; success={}.", res.is_ok());

    if let Err(DecryptError::Unknown(ref x)) = res {
        let body = str::from_utf8(&x.body).unwrap_or("<deserializing failed.>");
        warn!("DecryptError: {}", body);
    };

    let plaintext = res?.plaintext;
    let plaintext = plaintext.ok_or_else(|| Error::from(KmsError::NoPlainText))?;
    let plaintext = str::from_utf8(&plaintext)?;
    debug!("Successfully decrypted and read plain text.");

    Ok(plaintext.to_string())
}
