package com.demo.sslpinning.auth;

import com.fasterxml.jackson.annotation.JsonProperty;

public record AuthResponse(
        @JsonProperty("access_tokened") String accessToken,
        String tokenType,
        long expiresIn,
        String username) {}
