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
                new SecretItem("1", "sk_demo_7f3c9a2b1e8d4c6f", "high"),
                new SecretItem("2", "svc.internal.rotating.example", "high"),
                new SecretItem("3", "{\"premium\":true,\"region\":\"eu\"}", "medium"),
                new SecretItem("4", "SUP-88421-RESET", "medium"));
    }

    @GetMapping("/health")
    public Health health() {
        String gitSha = System.getenv("RENDER_GIT_COMMIT");
        if (gitSha == null || gitSha.isBlank()) {
            gitSha = System.getenv("GIT_COMMIT");
        }
        if (gitSha == null || gitSha.isBlank()) {
            gitSha = "unknown";
        }
        return new Health("ok", gitSha);
    }

    public record Health(String status, String gitSha) {}
}
