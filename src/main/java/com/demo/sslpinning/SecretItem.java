package com.demo.sslpinning;

import com.fasterxml.jackson.annotation.JsonInclude;

public record SecretItem(
        String id,
        @JsonInclude(JsonInclude.Include.NON_NULL) String title,
        String value,
        String sensitivity) {}
