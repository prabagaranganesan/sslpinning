//
//  AuthTokenStore.swift
//  SSLPinning
//

import Foundation
import Security

enum AuthTokenStore {
    private static let service = "com.demo.sslpinning"
    private static let account = "accessToken"
    private static let usernameAccount = "username"

    static func save(accessToken: String, username: String) {
        save(account: account, value: accessToken)
        save(account: usernameAccount, value: username)
    }

    static func loadAccessToken() -> String? {
        load(account: account)
    }

    static func loadUsername() -> String? {
        load(account: usernameAccount)
    }

    static func clear() {
        delete(account: account)
        delete(account: usernameAccount)
    }

    private static func save(account: String, value: String) {
        guard let data = value.data(using: .utf8) else { return }
        delete(account: account)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        SecItemAdd(query as CFDictionary, nil)
    }

    private static func load(account: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess, let data = item as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private static func delete(account: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
    }
}
