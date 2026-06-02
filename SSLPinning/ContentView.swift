//
//  ContentView.swift
//  SSLPinning
//

import SwiftUI

struct ContentView: View {
    @State private var model = SecretsViewModel()

    var body: some View {
        NavigationStack {
            List {
                Section {
                    TextField("Base URL", text: $model.baseURLString)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)

                    if model.isLoggedIn {
                        LabeledContent("Signed in", value: model.loggedInUsername ?? "")
                        Button(role: .destructive) {
                            model.logout()
                        } label: {
                            Label("Sign out", systemImage: "rectangle.portrait.and.arrow.right")
                        }
                    } else {
                        TextField("Username", text: $model.username)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        SecureField("Password", text: $model.password)
                        Button {
                            Task { await model.login() }
                        } label: {
                            if model.isAuthenticating {
                                Label("Signing in…", systemImage: "person.crop.circle.badge.checkmark")
                            } else {
                                Label("Sign in", systemImage: "person.crop.circle")
                            }
                        }
                        .disabled(model.isAuthenticating)
                    }
                } header: {
                    Text("Authentication")
                } footer: {
                    Text("POST /api/auth/login returns a JWT access token. Protected routes (e.g. GET /api/secrets) require `Authorization: Bearer <token>`. Demo user: demo / demo123.")
                }

                Section {
                    Toggle("SSL certificate pinning", isOn: $model.pinningEnabled)
                    TextField("Leaf SHA-256 (hex, manual)", text: $model.userPinnedLeafHex, axis: .vertical)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .font(.body.monospaced())
                        .lineLimit(3 ... 6)
                    Button {
                        Task { await model.load() }
                    } label: {
                        if model.isLoading {
                            Label("Loading…", systemImage: "arrow.triangle.2.circlepath")
                        } else {
                            Label("Load secrets", systemImage: "arrow.down.circle")
                        }
                    }
                    .disabled(model.isLoading || !model.isLoggedIn)
                } header: {
                    Text("Demo controls")
                } footer: {
                    Text(
                        "Get the pin yourself (Terminal): openssl s_client -connect YOUR_HOST:443 -servername YOUR_HOST </dev/null 2>/dev/null | openssl x509 -outform der | openssl dgst -sha256 — paste the hex here or into PinningConfig.pinnedLeafCertificateSHA256Hex in Xcode.\n\nProxy tools: use LAN IP or a public https URL, not 127.0.0.1, if you need traffic through a proxy."
                    )
                }

                if let msg = model.statusMessage {
                    Section("What happened") {
                        Text(msg)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                }

                Section("Secret items (from API)") {
                    if model.items.isEmpty, !model.isLoading {
                        Text(model.isLoggedIn
                            ? "No items yet. Tap Load secrets."
                            : "Sign in, then tap Load secrets.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(model.items) { item in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(item.title)
                                .font(.headline)
                            Text(item.value)
                                .font(.body.monospaced())
                            Text(item.sensitivity.uppercased())
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 4)
                    }
                }
            }
            .navigationTitle("SSL Pinning Demo")
        }
        .task {
            if model.isLoggedIn {
                await model.load()
            }
        }
    }
}

#Preview {
    ContentView()
}
