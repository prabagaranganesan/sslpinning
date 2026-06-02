package com.demo.sslpinning;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.boot.context.properties.EnableConfigurationProperties;

import com.demo.sslpinning.auth.JwtProperties;

@SpringBootApplication
@EnableConfigurationProperties(JwtProperties.class)
public class SslPinningApiApplication {

    public static void main(String[] args) {
        SpringApplication.run(SslPinningApiApplication.class, args);
    }
}
