//
//  SecretsAPIClient.swift
//  SSLPinning
//

import Foundation

enum SecretsAPIError: LocalizedError {
    case invalidURL
    case badStatus(Int)
    case unauthorized
    case decoding(Error)
    case transport(Error)

    var errorDescription: String? {
        switch self {
        case .invalidURL:
            return "The base URL is not valid."
        case let .badStatus(code):
            return "Server returned HTTP \(code)."
        case .unauthorized:
            return "Not authorized. Sign in again to refresh your access token."
        case let .decoding(err):
            return "Could not parse JSON: \(err.localizedDescription)"
        case let .transport(err):
            return err.localizedDescription
        }
    }
}

actor SecretsAPIClient {
    private let pinningDelegate = PinningSessionDelegate()
    private var session: URLSession?
    private var lastPinningEnabled = false
    private var lastPins: [String] = []
    private var accessToken: String?

    private func makeSession() -> URLSession {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 30
        return URLSession(configuration: config, delegate: pinningDelegate, delegateQueue: nil)
    }

    func setAccessToken(_ token: String?) {
        accessToken = token
    }

    func setPinning(enabled: Bool, pinnedHex: [String]) {
        let normalizedPins = pinnedHex.map { $0.lowercased() }.sorted()
        let changed = (enabled != lastPinningEnabled) || (normalizedPins != lastPins)

        pinningDelegate.pinningEnabled = enabled
        pinningDelegate.pinnedLeafCertificateSHA256Hex = normalizedPins
        lastPinningEnabled = enabled
        lastPins = normalizedPins

        if changed {
            session?.invalidateAndCancel()
            session = nil
        }
    }

    func login(baseURLString: String, username: String, password: String) async throws -> AuthResponse {
        let trimmed = baseURLString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let base = URL(string: trimmed),
              let url = URL(string: "/api/auth/login", relativeTo: base)?.absoluteURL
        else {
            throw SecretsAPIError.invalidURL
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.httpBody = try JSONEncoder().encode(LoginRequest(username: username, password: password))

        let data = try await performRequest(request)
        let decoder = JSONDecoder()
        do {
            return try decoder.decode(AuthResponse.self, from: data)
        } catch {
            throw SecretsAPIError.decoding(error)
        }
    }

    func fetchSecrets(baseURLString: String) async throws -> [SecretItem] {
        let trimmed = baseURLString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let base = URL(string: trimmed),
              let url = URL(string: "/api/secrets", relativeTo: base)?.absoluteURL
        else {
            throw SecretsAPIError.invalidURL
        }

        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        applyAuthorizationHeader(to: &request)

        let data = try await performRequest(request)
        let decoder = JSONDecoder()
        do {
            return try decoder.decode([SecretItem].self, from: data)
        } catch {
            throw SecretsAPIError.decoding(error)
        }
    }

    private func applyAuthorizationHeader(to request: inout URLRequest) {
        if let accessToken, !accessToken.isEmpty {
            request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        }
    }

    private func performRequest(_ request: URLRequest) async throws -> Data {
        let data: Data
        let response: URLResponse
        do {
            if session == nil {
                session = makeSession()
            }
            guard let session else {
                throw SecretsAPIError.transport(URLError(.unknown))
            }
            (data, response) = try await session.data(for: request)
        } catch {
            throw SecretsAPIError.transport(error)
        }

        guard let http = response as? HTTPURLResponse else {
            throw SecretsAPIError.transport(URLError(.badServerResponse))
        }

        switch http.statusCode {
        case 200 ..< 300:
            return data
        case 401, 403:
            throw SecretsAPIError.unauthorized
        default:
            throw SecretsAPIError.badStatus(http.statusCode)
        }
    }
}
