package com.demo.sslpinning;

import java.util.List;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api")
public class SecretsController {

    @GetMapping("/secrets")
    public List<SecretItem> secrets() {
        return List.of(
                new SecretItem("1", "API key (demo)", "sk_demo_7f3c9a2b1e8d4c6f", "high"),
                new SecretItem("2", "Internal service token", "svc.internal.rotating.example", "high"),
                new SecretItem("3", "Feature flag payload", "{\"premium\":true,\"region\":\"eu\"}", "medium"),
                new SecretItem("4", "Support override code", "SUP-88421-RESET", "medium"));
    }

    @GetMapping("/health")
    public Health health() {
        return new Health("ok");
    }

    public record Health(String status) {}
}
