//! Listener startup, shared ownership, and credential minting for the proxy.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::serve;
use super::types::{
    CredentialRegistry, ProxyEndpoint, ProxyHandle, UpstreamCredential, OPENROUTER_ROOT,
    UPSTREAM_URL_ENV,
};

impl ProxyHandle {
    /// Bind an ephemeral loopback port and start serving.
    ///
    /// `upstream_root` is the OpenRouter API root; tests pass a local mock so the
    /// suite stays offline. The listener owns a dedicated thread and runtime,
    /// making this synchronous entry point safe inside existing runtimes.
    pub fn start(upstream_root: String) -> std::io::Result<Self> {
        let registry = Arc::new(Mutex::new(CredentialRegistry::default()));
        let port = serve::spawn(upstream_root, Arc::clone(&registry))?;
        Ok(Self { port, registry })
    }

    /// Return the loopback port this proxy accepted on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Find or mint the endpoint that forwards `upstream_key` to OpenRouter.
    ///
    /// Token minting is idempotent per key, so repeated spawns do not grow the
    /// registry or expose distinct credentials for the same upstream secret.
    pub fn endpoint_for_key(&self, upstream_key: &str) -> ProxyEndpoint {
        self.endpoint_for_credential(upstream_key, &[])
    }

    /// Find or mint the endpoint forwarding `upstream_key` restricted to
    /// `provider_only`.
    ///
    /// Idempotency is per credential *and* pin: an unpinned run and a run
    /// pinned to `streamlake` get different tokens even on one OpenRouter key,
    /// which is what stops one preset's restriction from reaching another
    /// preset's traffic.
    pub fn endpoint_for_credential(
        &self,
        upstream_key: &str,
        provider_only: &[String],
    ) -> ProxyEndpoint {
        let credential = UpstreamCredential {
            key: upstream_key.to_string(),
            provider_only: provider_only.to_vec(),
        };
        let identity = credential.identity();
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(token) = registry.by_identity.get(&identity) {
            return ProxyEndpoint {
                port: self.port,
                token: token.clone(),
            };
        }

        // UUID v4 reads the operating system CSPRNG. This is a bearer token, so
        // predictable per-call randomness would let a local process redeem the
        // upstream key through the loopback listener.
        let token = format!("mdl-{}", random_nonce());
        registry.by_token.insert(token.clone(), credential);
        registry.by_identity.insert(identity, token.clone());
        ProxyEndpoint {
            port: self.port,
            token,
        }
    }
}

/// Produce 32 lowercase hexadecimal characters from the operating system CSPRNG.
fn random_nonce() -> String {
    let mut bytes = [0u8; 16];
    for chunk in bytes.chunks_mut(8) {
        let uuid = uuid::Uuid::new_v4();
        chunk.copy_from_slice(&uuid.as_bytes()[8..16]);
    }
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from_digit(u32::from(byte >> 4), 16).expect("nibble is valid"));
        output.push(char::from_digit(u32::from(byte & 0x0f), 16).expect("nibble is valid"));
    }
    output
}

/// The process-wide proxy, started on first use.
static SHARED: OnceLock<Result<ProxyHandle, String>> = OnceLock::new();

/// Resolve the proxy's upstream root, honoring the test-only environment override.
fn upstream_root(env: &HashMap<String, String>) -> String {
    env.get(UPSTREAM_URL_ENV)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(OPENROUTER_ROOT)
        .to_string()
}

/// Return the shared proxy, starting it if this is the first caller.
///
/// Startup outcomes are memoized. Repeatedly retrying a failed loopback bind at
/// every harness launch would obscure one actionable error with identical ones.
pub fn shared(env: &HashMap<String, String>) -> Result<&'static ProxyHandle, String> {
    SHARED
        .get_or_init(|| {
            ProxyHandle::start(upstream_root(env))
                .map_err(|error| format!("could not start the local attribution proxy: {error}"))
        })
        .as_ref()
        .map_err(String::clone)
}
