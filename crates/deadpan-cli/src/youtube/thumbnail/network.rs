//! HTTPS thumbnail transport using the shared bounded public-address resolver.

use ureq::unversioned::resolver::{ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::NextTimeout;

use crate::youtube::network::{dns, ip};

#[derive(Debug)]
pub(super) struct PublicResolver;

impl Resolver for PublicResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        _: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let host = uri.host().ok_or(ureq::Error::HostNotFound)?;
        if uri.scheme_str() != Some("https")
            || uri.port_u16().is_some_and(|port| port != 443)
            || !super::supported_host(host)
        {
            return Err(ureq::Error::HostNotFound);
        }
        let wait = if timeout.after.is_not_happening() {
            super::TIMEOUT
        } else {
            (*timeout.after).min(super::TIMEOUT)
        };
        let addresses = dns::resolve(host, wait).map_err(|error| match error {
            dns::LookupError::Timeout => ureq::Error::Timeout(timeout.reason),
            dns::LookupError::Unavailable => ureq::Error::HostNotFound,
        })?;
        let mut result = self.empty();
        for address in addresses.into_iter().take(16) {
            if address.port() == 443 && ip::public_address(address.ip()) {
                result.push(address);
            }
        }
        if result.is_empty() {
            Err(ureq::Error::HostNotFound)
        } else {
            Ok(result)
        }
    }
}
