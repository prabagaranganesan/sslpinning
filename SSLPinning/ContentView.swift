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
                    //Toggle("SSL certificate pinning", isOn: $model.pinningEnabled)
                    Button {
                        Task { await model.load() }
                    } label: {
                        if model.isLoading {
                            Label("Loading…", systemImage: "arrow.triangle.2.circlepath")
                        } else {
                            Label("Load secrets", systemImage: "arrow.down.circle")
                        }
                    }
                    .disabled(model.isLoading)
                } header: {
                    Text("Demo controls")
                } footer: {
                    Text(
                        "Proxy tools (e.g. ProxyHawk): use your Mac’s LAN IP in the URL, not 127.0.0.1—loopback bypasses the HTTP proxy. Point Simulator Settings → Wi‑Fi → HTTP Proxy at ProxyHawk, then use http://192.168.x.x:8080.\n\nWithout pinning, the app trusts the system roots. With pinning over HTTPS, the leaf certificate must match the hash in code."
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
                        Text("No items yet. Start the Spring Boot app and tap Load secrets.")
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
            await model.load()
        }
    }
}

#Preview {
    ContentView()
}
