//
//  PinningSessionDelegate.swift
//  SSLPinning
//

import CryptoKit
import Foundation
import Security

final class PinningSessionDelegate: NSObject, URLSessionDelegate {
    var pinningEnabled = true
    var pinnedLeafCertificateSHA256Hex: [String] = []

    func urlSession(
        _ session: URLSession,
        didReceive challenge: URLAuthenticationChallenge,
        completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
    ) {
        guard pinningEnabled,
              challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
              let serverTrust = challenge.protectionSpace.serverTrust
        else {
            completionHandler(.performDefaultHandling, nil)
            return
        }

        let proto = challenge.protectionSpace.protocol?.lowercased() ?? ""
        if proto == "http" {
            completionHandler(.performDefaultHandling, nil)
            return
        }

        if pinnedLeafCertificateSHA256Hex.isEmpty {
            completionHandler(.performDefaultHandling, nil)
            return
        }

        guard let chain = SecTrustCopyCertificateChain(serverTrust) as? [SecCertificate],
              let leaf = chain.first
        else {
            completionHandler(.cancelAuthenticationChallenge, nil)
            return
        }

        let der = SecCertificateCopyData(leaf) as Data
        let digest = SHA256.hash(data: der)
        let hex = digest.map { String(format: "%02x", $0) }.joined()

        if pinnedLeafCertificateSHA256Hex.contains(hex) {
            completionHandler(.useCredential, URLCredential(trust: serverTrust))
        } else {
            completionHandler(.cancelAuthenticationChallenge, nil)
        }
    }
}
