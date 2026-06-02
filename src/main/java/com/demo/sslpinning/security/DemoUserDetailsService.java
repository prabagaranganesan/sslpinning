package com.demo.sslpinning.security;

import org.springframework.beans.factory.annotation.Value;
import org.springframework.security.core.userdetails.User;
import org.springframework.security.core.userdetails.UserDetails;
import org.springframework.security.core.userdetails.UserDetailsService;
import org.springframework.security.core.userdetails.UsernameNotFoundException;
import org.springframework.security.crypto.password.PasswordEncoder;
import org.springframework.stereotype.Service;

@Service
public class DemoUserDetailsService implements UserDetailsService {

    private final String demoUsername;
    private final String encodedPassword;

    public DemoUserDetailsService(
            @Value("${app.demo.username}") String demoUsername,
            @Value("${app.demo.password}") String demoPassword,
            PasswordEncoder passwordEncoder) {
        this.demoUsername = demoUsername;
        this.encodedPassword = passwordEncoder.encode(demoPassword);
    }

    @Override
    public UserDetails loadUserByUsername(String username) throws UsernameNotFoundException {
        if (!demoUsername.equals(username)) {
            throw new UsernameNotFoundException("User not found");
        }
        return User.withUsername(demoUsername)
                .password(encodedPassword)
                .roles("USER")
                .build();
    }
}
