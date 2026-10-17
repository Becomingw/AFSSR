# TLS Fingerprint Capture

Arca stores per-request TLS capture under `usage.request_metadata.tls_fingerprint`.

```json
{
  "tls_fingerprint": {
    "incoming": {
      "source": "forwarded_header",
      "ja3": "...",
      "ja3_hash": "...",
      "ja4": "...",
      "protocol": "TLSv1.3",
      "cipher": "TLS_AES_128_GCM_SHA256",
      "sni": "api.example.com",
      "alpn": "h2"
    },
    "outgoing": {
      "source": "aether_transport_config",
      "observed": false,
      "transport_path": "direct",
      "backend": "reqwest_rustls",
      "http_mode": "auto",
      "tls_stack": "rustls",
      "tls_versions_offered": ["TLS1.3", "TLS1.2"],
      "alpn_offered": ["h2", "http/1.1"]
    }
  }
}
```

`incoming` is the client-to-Arca TLS fingerprint. It can be populated by Arca native TLS capture in direct deployments or by trusted reverse-proxy headers when TLS terminates before Arca.

`outgoing` is the Arca-to-provider TLS transport record. The current gateway records the exact transport configuration it controls. It sets `observed: false` because reqwest/rustls does not expose the emitted ClientHello bytes on the direct path. A future connector-level ClientHello capture or probe result can reuse the same object with `observed: true` plus `ja3`, `ja3_hash`, and `ja4`.

## Nginx TLS Termination

When nginx terminates HTTPS and proxies HTTP to Arca, Arca cannot see the original ClientHello. Configure nginx to forward the TLS fields it can observe:

```nginx
server {
    listen 443 ssl http2;
    server_name api.example.com;

    ssl_certificate     /etc/letsencrypt/live/api.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_http_version 1.1;

        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        proxy_set_header X-Aether-TLS-Source nginx;
        proxy_set_header X-Aether-TLS-Protocol $ssl_protocol;
        proxy_set_header X-Aether-TLS-Cipher $ssl_cipher;
        proxy_set_header X-Aether-TLS-SNI $ssl_server_name;
    }
}
```

Stock nginx does not provide JA3/JA4 variables. The forwarded record is still useful, but it is not a complete TLS fingerprint. To forward JA3/JA4 through nginx, use an nginx build/module or edge layer that computes them and set:

```nginx
proxy_set_header X-Aether-TLS-JA3      $ja3;
proxy_set_header X-Aether-TLS-JA3-Hash $ja3_hash;
proxy_set_header X-Aether-TLS-JA4      $ja4;
```

Only accept these headers from trusted infrastructure. Do not expose Aether directly to public clients while also trusting client-supplied `X-Aether-TLS-*` headers.

## Nginx TCP Passthrough

If Arca terminates TLS itself, nginx can pass TCP through without decrypting:

```nginx
stream {
    map $ssl_preread_server_name $aether_backend {
        api.example.com 127.0.0.1:3443;
        default         127.0.0.1:3443;
    }

    server {
        listen 443;
        proxy_pass $aether_backend;
        ssl_preread on;
    }
}
```

In this mode nginx cannot inject HTTP headers because it never sees HTTP. Arca native TLS capture is responsible for populating `tls_fingerprint.incoming`.
