//
//  SecretsAPIClient.swift
//  SSLPinning
//

import Foundation

enum SecretsAPIError: LocalizedError {
    case invalidURL
    case badStatus(Int)
    case decoding(Error)
    case transport(Error)

    var errorDescription: String? {
        switch self {
        case .invalidURL:
            return "The base URL is not valid."
        case let .badStatus(code):
            return "Server returned HTTP \(code)."
        case let .decoding(err):
            return "Could not parse JSON: \(err.localizedDescription)"
        case let .transport(err):
            return err.localizedDescription
        }
    }
}

actor SecretsAPIClient {
    private let pinningDelegate = PinningSessionDelegate()
    private lazy var session: URLSession = makeSession()

    private func makeSession() -> URLSession {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 30
        return URLSession(configuration: config, delegate: pinningDelegate, delegateQueue: nil)
    }

    func setPinning(enabled: Bool, pinnedHex: [String]) {
        pinningDelegate.pinningEnabled = enabled
        pinningDelegate.pinnedLeafCertificateSHA256Hex = pinnedHex.map { $0.lowercased() }
    }

    func fetchSecrets(baseURLString: String) async throws -> [SecretItem] {
        let trimmed = baseURLString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let base = URL(string: trimmed), let url = URL(string: "/api/secrets", relativeTo: base)?.absoluteURL else {
            throw SecretsAPIError.invalidURL
        }

        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            throw SecretsAPIError.transport(error)
        }

        guard let http = response as? HTTPURLResponse else {
            throw SecretsAPIError.transport(URLError(.badServerResponse))
        }
        guard (200 ..< 300).contains(http.statusCode) else {
            throw SecretsAPIError.badStatus(http.statusCode)
        }

        let decoder = JSONDecoder()
        do {
            return try decoder.decode([SecretItem].self, from: data)
        } catch {
            throw SecretsAPIError.decoding(error)
        }
    }
}
