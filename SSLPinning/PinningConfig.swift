//
//  PinningConfig.swift
//  SSLPinning
//

import Foundation

enum PinningConfig {
    /// Local: `http://127.0.0.1:8080` or your Mac LAN IP (needed for ProxyHawk). Render: `https://<your-service>.onrender.com`
    /// Loopback bypasses the HTTP proxy; use LAN IP or a public HTTPS URL for proxy tools.
    static let defaultBaseURLString = "https://sslpinning-api.onrender.com"

    /// Physical device: same—use `http://<Mac-LAN-IP>:8080`. Spring Boot is configured with `server.address=0.0.0.0`.
    /// `NSAllowsLocalNetworking` allows cleartext HTTP to the LAN for this demo.

    /// After you serve the API over HTTPS, pin the **leaf** certificate's DER bytes:
    /// `openssl s_client -connect host:443 -servername host </dev/null 2>/dev/null | openssl x509 -outform der | openssl dgst -sha256`
    /// Paste the lowercase hex digest (no colons) here. Wrong or missing pin → connection fails when pinning is on.
    static let pinnedLeafCertificateSHA256Hex: [String] = []
}
