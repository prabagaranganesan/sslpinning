package com.demo.sslpinning.auth;

public record AuthResponse(
        String accessToken,
        String tokenType,
        long expiresIn,
        String username) {}
