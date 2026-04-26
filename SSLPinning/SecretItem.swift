//
//  SecretItem.swift
//  SSLPinning
//

import Foundation

struct SecretItem: Identifiable, Codable, Hashable {
    let id: String
    let title: String
    let value: String
    let sensitivity: String
}
