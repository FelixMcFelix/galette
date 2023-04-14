use std::sync::Arc;

use crp::{CrpSource, InMemory, Response};
use rcgen::{Certificate, CertificateParams, CustomExtension, DistinguishedName, DnType, SanType};
use rustls::{
	client::{ServerCertVerified, ServerCertVerifier},
	server::{ClientCertVerified, ClientCertVerifier},
	ServerName,
};
use serde::{Deserialize, Serialize};

const RESP_LEN: usize = 256 / 8;
const CHALLENGE_OID: [u64; 9] = [1, 3, 6, 1, 5, 5, 7, 13, 0];

#[derive(Deserialize, Serialize)]
pub struct KeySource<T> {
	crps: T,
}

impl KeySource<InMemory<[u8; RESP_LEN]>> {
	pub fn new_random() -> Self {
		Self {
			crps: crp::InMemory::new_random(std::u16::MAX as usize),
		}
	}
}

impl<T> KeySource<T>
where
	T: CrpSource<Challenge = u64, Response = [u8; RESP_LEN]>,
{
	pub fn gen_cert(&self) -> Certificate {
		let mut cfg = CertificateParams::default();
		let challenge: u64 = 1045;

		let a = self.crps.respond(challenge);

		match a {
			Response::Unused(a) => {
				cfg.aux_enc_data = Some(a.to_vec());
				cfg.custom_extensions
					.push(CustomExtension::from_oid_content(
						&CHALLENGE_OID,
						challenge.to_le_bytes().to_vec(),
					));

				cfg.not_before = rcgen::date_time_ymd(1975, 1, 1);
				cfg.not_after = rcgen::date_time_ymd(4096, 1, 1);
				cfg.distinguished_name = DistinguishedName::new();
				cfg.distinguished_name
					.push(DnType::OrganizationName, "TruSDEd CRP Cert");
				cfg.distinguished_name
					.push(DnType::CommonName, "Master Cert");
				cfg.subject_alt_names = vec![
					// SanType::DnsName("crabs.crabs".to_string()),
					SanType::DnsName("localhost".to_string()),
				];
			},
			e => {
				unimplemented!("Handling not impl'd for {:?}", e);
			},
		}

		Certificate::from_params(cfg).expect("hmm")
	}

	pub fn inner(&self) -> &T {
		&self.crps
	}
}

pub struct CrpClientTlsVerifier<T> {
	pub base: Arc<dyn ClientCertVerifier>,
	pub crps: KeySource<T>,
}

pub struct CrpServerTlsVerifier<T> {
	pub base: Arc<dyn ServerCertVerifier>,
	pub crps: KeySource<T>,
}

impl<T> ClientCertVerifier for CrpClientTlsVerifier<T>
where
	T: CrpSource<Challenge = u64, Response = [u8; RESP_LEN]> + Send + Sync,
{
	fn client_auth_root_subjects(&self) -> Option<rustls::DistinguishedNames> {
		self.base.client_auth_root_subjects()
	}

	fn verify_client_cert(
		&self,
		end_entity: &rustls::Certificate,
		intermediates: &[rustls::Certificate],
		now: std::time::SystemTime,
	) -> Result<ClientCertVerified, rustls::Error> {
		let challenge = {
			let (_remainder, cert) = x509_parser::parse_x509_certificate(&end_entity.0)
				.map_err(|_| rustls::Error::InvalidCertificateEncoding)?;

			let wanted_oid =
				asn1_rs::Oid::from(&CHALLENGE_OID).expect("Should be fine as an extension val.");

			// if we find an extension with our OID, then we verify locally.
			// Else, delegate to PSK via self.base.
			let out = cert
				.tbs_certificate
				.iter_extensions()
				.find(|v| v.oid == wanted_oid)
				.map(|v| v.value)
				.and_then(|v| v.try_into().ok().map(u64::from_le_bytes));

			out
		};

		if let Some(challenge) = challenge {
			let webpki_now =
				webpki::Time::try_from(now).map_err(|_| rustls::Error::FailedToGetCurrentTime)?;

			let cert = webpki::EndEntityCert::try_from(end_entity.0.as_ref()).map_err(pki_error)?;
			let resp = self.crps.inner().respond(challenge);

			let resp = if let Response::Unused(resp) = resp {
				resp
			} else {
				todo!();
			};

			let ta = webpki::TrustAnchor::try_from_cert_der(&end_entity.0).unwrap();

			cert.verify_is_valid_tls_client_cert_with_aux_data(
				SUPPORTED_SIG_ALGS,
				&webpki::TlsClientTrustAnchors(&[ta]),
				&[],
				webpki_now,
				&resp,
			)
			.map_err(pki_error)
			.map(|_| ClientCertVerified::assertion())
		} else {
			self.base.verify_client_cert(end_entity, intermediates, now)
		}
	}
}

impl<T> ServerCertVerifier for CrpServerTlsVerifier<T>
where
	T: CrpSource<Challenge = u64, Response = [u8; RESP_LEN]> + Send + Sync,
{
	fn verify_server_cert(
		&self,
		end_entity: &rustls::Certificate,
		intermediates: &[rustls::Certificate],
		server_name: &rustls::ServerName,
		scts: &mut dyn Iterator<Item = &[u8]>,
		ocsp_response: &[u8],
		now: std::time::SystemTime,
	) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
		let challenge = {
			let (_remainder, cert) = x509_parser::parse_x509_certificate(&end_entity.0)
				.map_err(|_| rustls::Error::InvalidCertificateEncoding)?;

			let wanted_oid =
				asn1_rs::Oid::from(&CHALLENGE_OID).expect("Should be fine as an extension val.");

			// if we find an extension with our OID, then we verify locally.
			// Else, delegate to PSK via self.base.
			let out = cert
				.tbs_certificate
				.iter_extensions()
				.find(|v| v.oid == wanted_oid)
				.map(|v| v.value)
				.and_then(|v| v.try_into().ok().map(u64::from_le_bytes));

			out
		};

		if let Some(challenge) = challenge {
			let webpki_now =
				webpki::Time::try_from(now).map_err(|_| rustls::Error::FailedToGetCurrentTime)?;

			let cert = webpki::EndEntityCert::try_from(end_entity.0.as_ref()).map_err(pki_error)?;
			let resp = self.crps.inner().respond(challenge);

			let resp = if let Response::Unused(resp) = resp {
				resp
			} else {
				todo!();
			};

			let ta = webpki::TrustAnchor::try_from_cert_der(&end_entity.0).unwrap();

			let dns_name = match server_name {
				ServerName::DnsName(dns_name) => dns_name,
				ServerName::IpAddress(_) => {
					return Err(rustls::Error::UnsupportedNameType);
				},
				_ => todo!(),
			};

			let cert = cert
				.verify_is_valid_tls_server_cert_with_aux_data(
					SUPPORTED_SIG_ALGS,
					&webpki::TlsServerTrustAnchors(&[ta]),
					&[],
					webpki_now,
					&resp,
				)
				.map_err(pki_error)
				.map(|_| cert)?;

			// if let Some(policy) = &self.ct_policy {
			//     policy.verify(end_entity, now, scts)?;
			// }

			// if !ocsp_response.is_empty() {
			//     trace!("Unvalidated OCSP response: {:?}", ocsp_response.to_vec());
			// }

			cert.verify_is_valid_for_dns_name(
				webpki::DnsNameRef::try_from_ascii_str(dns_name.as_ref()).expect("aaaa"),
			)
			.map_err(pki_error)
			.map(|_| ServerCertVerified::assertion())
		} else {
			self.base.verify_server_cert(
				end_entity,
				intermediates,
				server_name,
				scts,
				ocsp_response,
				now,
			)
		}
	}
}

// Copied from rustls/src/verify.rs
fn pki_error(error: webpki::Error) -> rustls::Error {
	use webpki::Error::*;
	match error {
		BadDer | BadDerTime => rustls::Error::InvalidCertificateEncoding,
		InvalidSignatureForPublicKey => rustls::Error::InvalidCertificateSignature,
		UnsupportedSignatureAlgorithm | UnsupportedSignatureAlgorithmForPublicKey =>
			rustls::Error::InvalidCertificateSignatureType,
		e => rustls::Error::InvalidCertificateData(format!("invalid peer certificate: {e}")),
	}
}

static SUPPORTED_SIG_ALGS: &[&webpki::SignatureAlgorithm] = &[
	&webpki::ECDSA_P256_SHA256,
	&webpki::ECDSA_P256_SHA384,
	&webpki::ECDSA_P384_SHA256,
	&webpki::ECDSA_P384_SHA384,
	&webpki::ED25519,
	&webpki::RSA_PSS_2048_8192_SHA256_LEGACY_KEY,
	&webpki::RSA_PSS_2048_8192_SHA384_LEGACY_KEY,
	&webpki::RSA_PSS_2048_8192_SHA512_LEGACY_KEY,
	&webpki::RSA_PKCS1_2048_8192_SHA256,
	&webpki::RSA_PKCS1_2048_8192_SHA384,
	&webpki::RSA_PKCS1_2048_8192_SHA512,
	&webpki::RSA_PKCS1_3072_8192_SHA384,
];
