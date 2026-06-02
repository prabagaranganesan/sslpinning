//
//  SecretsViewModel.swift
//  SSLPinning
//

import Foundation
import Observation

private let userPinnedLeafHexKey = "sslPinning.userLeafHex"

@Observable
final class SecretsViewModel {
    var baseURLString: String
    var username: String
    var password: String
    var loggedInUsername: String?
    /// When ON, HTTPS uses custom pinning; HTTP still uses default handling (no cert to pin).
    var pinningEnabled = false
    /// Optional manual paste (same format as `PinningConfig.pinnedLeafCertificateSHA256Hex`). Comma/space-separated allowed.
    var userPinnedLeafHex: String {
        didSet { UserDefaults.standard.set(userPinnedLeafHex, forKey: userPinnedLeafHexKey) }
    }

    var items: [SecretItem] = []
    var statusMessage: String?
    var isLoading = false
    var isAuthenticating = false

    var isLoggedIn: Bool { loggedInUsername != nil }

    private let client = SecretsAPIClient()

    init(baseURLString: String = PinningConfig.defaultBaseURLString) {
        self.baseURLString = baseURLString
        self.username = PinningConfig.defaultUsername
        self.password = PinningConfig.defaultPassword
        userPinnedLeafHex = UserDefaults.standard.string(forKey: userPinnedLeafHexKey) ?? ""
        loggedInUsername = AuthTokenStore.loadUsername()
    }

    private func effectivePins() -> [String] {
        var set = Set<String>()
        let pieces = userPinnedLeafHex
            .replacingOccurrences(of: ":", with: "")
            .split { $0.isWhitespace || $0 == "," }
            .map { String($0).lowercased() }
            .filter { !$0.isEmpty }
        pieces.forEach { set.insert($0) }
        PinningConfig.pinnedLeafCertificateSHA256Hex.forEach { set.insert($0.lowercased()) }
        return Array(set)
    }

    @MainActor
    private func configureClient() async {
        let pins = pinningEnabled ? effectivePins() : []
        await client.setPinning(enabled: pinningEnabled, pinnedHex: pins)
        await client.setAccessToken(AuthTokenStore.loadAccessToken())
    }

    @MainActor
    func login() async {
        isAuthenticating = true
        statusMessage = nil
        defer { isAuthenticating = false }

        await configureClient()

        do {
            let auth = try await client.login(
                baseURLString: baseURLString,
                username: username.trimmingCharacters(in: .whitespacesAndNewlines),
                password: password
            )
            AuthTokenStore.save(accessToken: auth.accessToken, username: auth.username)
            await client.setAccessToken(auth.accessToken)
            loggedInUsername = auth.username
            statusMessage = "Signed in as \(auth.username). Access token expires in \(auth.expiresIn)s."
            await load()
        } catch {
            loggedInUsername = nil
            AuthTokenStore.clear()
            await client.setAccessToken(nil)
            items = []
            statusMessage = error.localizedDescription
        }
    }

    @MainActor
    func logout() {
        AuthTokenStore.clear()
        loggedInUsername = nil
        items = []
        statusMessage = "Signed out."
        Task { await client.setAccessToken(nil) }
    }

    @MainActor
    func load() async {
        guard isLoggedIn else {
            items = []
            statusMessage = "Sign in first. `/api/secrets` requires a Bearer access token."
            return
        }

        isLoading = true
        statusMessage = nil
        defer { isLoading = false }

        await configureClient()

        let scheme = URL(string: baseURLString.trimmingCharacters(in: .whitespacesAndNewlines))?.scheme?.lowercased() ?? ""

        do {
            items = try await client.fetchSecrets(baseURLString: baseURLString)

            if pinningEnabled, scheme == "http" {
                statusMessage =
                    "Fetched over HTTP. Pinning does not apply (no TLS). Use https:// with a pin from OpenSSL or PinningConfig.swift."
            } else if pinningEnabled, effectivePins().isEmpty, scheme == "https" {
                statusMessage =
                    "Pinning is ON and strict mode is active. Because no pin is set, HTTPS requests are blocked. Add the leaf SHA-256 (hex) in the field below or in PinningConfig.pinnedLeafCertificateSHA256Hex."
            } else if pinningEnabled, scheme == "https", !effectivePins().isEmpty {
                statusMessage =
                    "Pinning enforced: the server leaf must match your pin. Wrong hash or a proxy MITM → failure."
            } else if !pinningEnabled, scheme == "https" {
                statusMessage =
                    "Pinning OFF: normal TLS trust (system roots + any user-installed proxy CA)."
            } else {
                statusMessage = "Loaded over \(scheme.uppercased()). Enable pinning + HTTPS + a manual pin to compare behavior."
            }
        } catch SecretsAPIError.unauthorized {
            logout()
            statusMessage = SecretsAPIError.unauthorized.errorDescription
        } catch {
            items = []
            if pinningEnabled, scheme == "https", effectivePins().isEmpty {
                statusMessage =
                    "Pinning blocked the request because no pin is configured. Set the leaf SHA-256 hex and retry."
            } else if pinningEnabled, scheme == "https", !effectivePins().isEmpty {
                statusMessage =
                    "\(error.localizedDescription)\n\nIf the pin does not match the server leaf (or TLS failed), pinning blocks the connection."
            } else {
                statusMessage = error.localizedDescription
            }
        }
    }
}
