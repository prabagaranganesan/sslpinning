//
//  PinningConfig.swift
//  SSLPinning
//

import Foundation

enum PinningConfig {
    /// Local: `http://127.0.0.1:8090` or your Mac LAN IP (needed for ProxyHawk). Render: `https://<your-service>.onrender.com`
    /// Loopback bypasses the HTTP proxy; use LAN IP or a public HTTPS URL for proxy tools.
    static let defaultBaseURLString = "https://sslpinning-api.onrender.com"

    /// Physical device: same—use `http://<Mac-LAN-IP>:8090`. Spring Boot is configured with `server.address=0.0.0.0`.
    /// `NSAllowsLocalNetworking` allows cleartext HTTP to the LAN for this demo.

    /// Manual pin (compile-time): SHA-256 of the leaf certificate DER (lowercase hex, no colons).
    /// OpenSSL (replace HOST): openssl s_client -connect HOST:443 -servername HOST </dev/null 2>/dev/null | openssl x509 -outform der | openssl dgst -sha256
    /// Use the hex from the digest line. Same value can be pasted in the app’s “Leaf SHA-256” field.
    static let pinnedLeafCertificateSHA256Hex: [String] = []

    /// Default demo credentials (match backend `app.demo.*` properties).
    static let defaultUsername = "demo"
    static let defaultPassword = "demo123"
}
