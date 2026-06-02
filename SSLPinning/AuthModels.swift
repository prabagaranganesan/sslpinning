//
//  AuthModels.swift
//  SSLPinning
//

import Foundation

struct LoginRequest: Encodable {
    let username: String
    let password: String
}

struct AuthResponse: Decodable {
    let accessToken: String
    let tokenType: String
    let expiresIn: Int
    let username: String
}
