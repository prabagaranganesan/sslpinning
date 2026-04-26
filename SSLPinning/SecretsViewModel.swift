//
//  SecretsViewModel.swift
//  SSLPinning
//

import Foundation
import Observation

@Observable
final class SecretsViewModel {
    var baseURLString: String
    var pinningEnabled = true
    var items: [SecretItem] = []
    var statusMessage: String?
    var isLoading = false

    private let client = SecretsAPIClient()

    init(baseURLString: String = PinningConfig.defaultBaseURLString) {
        self.baseURLString = baseURLString
    }

    @MainActor
    func load() async {
        isLoading = true
        statusMessage = nil
        defer { isLoading = false }

        let pins = pinningEnabled ? PinningConfig.pinnedLeafCertificateSHA256Hex : []
        await client.setPinning(enabled: pinningEnabled, pinnedHex: pins)

        do {
            items = try await client.fetchSecrets(baseURLString: baseURLString)
            if pinningEnabled, URL(string: baseURLString.trimmingCharacters(in: .whitespacesAndNewlines))?.scheme?.lowercased() == "http" {
                statusMessage =
                    "Fetched over HTTP. There is no TLS certificate on this connection, so pinning cannot prove server identity—only HTTPS + a configured pin does."
            } else if pinningEnabled, PinningConfig.pinnedLeafCertificateSHA256Hex.isEmpty,
                      URL(string: baseURLString.trimmingCharacters(in: .whitespacesAndNewlines))?.scheme?.lowercased() == "https"
            {
                statusMessage =
                    "Pinning is on but `PinningConfig.pinnedLeafCertificateSHA256Hex` is empty—add your leaf cert SHA-256 (hex) after enabling HTTPS on the server."
            } else {
                statusMessage = pinningEnabled
                    ? "Pinning is on. If the leaf certificate does not match the configured hash, this request would fail."
                    : "Standard TLS trust (or HTTP). A proxy could present another valid certificate and the app would still accept it unless you pin."
            }
        } catch {
            items = []
            statusMessage = error.localizedDescription
        }
    }
}
