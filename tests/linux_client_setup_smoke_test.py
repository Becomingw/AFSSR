"""Exercise Unix installers with fake clients, loopback HTTP and synthetic keys only."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import threading
import tomllib


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "apps/aether-gateway/src/handlers/public/support/install.rs"
TEMPLATE = SOURCE.read_text().split("fn build_unix_script(", 1)[1].split('r###"', 1)[1].split('"###,', 1)[0]
KEY = "sk-synthetic-offline-fixture"
OLD_KEY = "sk-synthetic-old-header-fixture"
ROTATED_KEY = "sk-synthetic-rotated-fixture"
BASE = "https://gateway.example.invalid"


def render_script(target, binary, package, base=BASE, system="linux", api_key=KEY):
    return TEMPLATE.format_map({
        "target_cli": target,
        "target_system": system,
        "base_url": shlex.quote(base),
        "api_key": shlex.quote(api_key),
        "label": shlex.quote(binary),
        "binary": shlex.quote(binary),
        "npm_package": shlex.quote(package),
    })


def test_existing_clients():
    with tempfile.TemporaryDirectory(prefix="afssr-linux-client-") as directory:
        home = Path(directory)
        binaries = home / "fake-bin"
        binaries.mkdir()
        (binaries / "python3").symlink_to(sys.executable)
        for name, text in {
            "uname": "printf 'Linux\\n'",
            "codex": "exit 0",
            "claude": "exit 0",
            "npm": "echo 'Unexpected package installation' >&2; exit 77",
            "curl": "echo 'Unexpected network request' >&2; exit 77",
            "wget": "echo 'Unexpected network request' >&2; exit 77",
        }.items():
            path = binaries / name
            path.write_text(f"#!/bin/sh\n{text}\n")
            path.chmod(0o700)
        (home / ".codex").mkdir()
        config = home / ".codex/config.toml"
        config.write_text('model = "fixture-model"\napproval_policy = "on-request"\n[model_providers.keep]\nname = "Keep"\nbase_url = "https://keep.example.invalid/v1"\n')
        auth = home / ".codex/auth.json"
        auth.write_text('{"fixture":"untouched"}')
        (home / ".claude").mkdir()
        claude = home / ".claude/settings.json"
        claude.write_text(json.dumps({"env": {"KEEP_ME": "yes"}, "permissions": {"allow": ["Read"]}}))
        environment = {**os.environ, "HOME": str(home), "PATH": f"{binaries}:{os.environ['PATH']}"}
        for target, binary, package in [
            ("codex_cli", "codex", "@openai/codex"),
            ("claude_code", "claude", "@anthropic-ai/claude-code"),
        ]:
            script = render_script(target, binary, package)
            for _ in range(2):
                result = subprocess.run(["sh"], input=script, text=True, capture_output=True, env=environment, timeout=15)
                assert result.returncode == 0, result.stderr
                assert KEY not in result.stdout + result.stderr
            path = config if target == "codex_cli" else claude
            assert path.stat().st_mode & 0o777 == 0o600
        parsed = tomllib.loads(config.read_text())
        assert parsed["model_provider"] == "aether"
        assert parsed["model"] == "fixture-model"
        assert parsed["approval_policy"] == "on-request"
        assert parsed["model_providers"]["keep"]["name"] == "Keep"
        gateway = parsed["model_providers"]["aether"]
        assert gateway["base_url"] == BASE + "/v1"
        assert gateway["wire_api"] == "responses"
        assert gateway["experimental_bearer_token"] == KEY
        assert gateway["requires_openai_auth"] is False
        assert auth.read_text() == '{"fixture":"untouched"}'
        settings = json.loads(claude.read_text())
        assert settings["env"] == {"KEEP_ME": "yes", "ANTHROPIC_BASE_URL": BASE, "ANTHROPIC_AUTH_TOKEN": KEY}
        assert settings["permissions"] == {"allow": ["Read"]}
        assert (home / ".aether/client.env").stat().st_mode & 0o777 == 0o600


class CatalogHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.server.requests.append((self.path, self.headers.get("Authorization")))
        self.send_response(self.server.status)
        if self.server.status == 302:
            self.send_header("Location", self.server.base + "/must-not-follow")
        self.end_headers()
        self.wfile.write(self.server.payload)

    def log_message(self, *_args):
        pass


def test_opencode():
    server = ThreadingHTTPServer(("127.0.0.1", 0), CatalogHandler)
    server.base = f"http://127.0.0.1:{server.server_port}"
    server.requests = []
    server.status = 200
    remote_provider = {
        "npm": "@ai-sdk/openai-compatible", "name": "Aether",
        "options": {"baseURL": server.base + "/v1"},
        "models": {"fixture": {
            "name": "Fixture", "limit": {"context": 200000, "output": 12000},
            "tool_call": True, "reasoning": True,
            "modalities": {"input": ["text", "image"], "output": ["text"]},
            "cost": {"input": 1.2, "output": 3.4, "cache_read": 0.12},
        }},
    }
    server.payload = json.dumps({"provider": {"aether": remote_provider}}).encode()
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix="afssr-opencode-") as directory:
            home = Path(directory)
            binaries = home / "fake-bin"
            binaries.mkdir()
            (binaries / "python3").symlink_to(sys.executable)
            for binary, content in {
                "uname": "printf 'Linux\\n'", "opencode": "exit 0",
                "npm": "exit 77", "curl": "exit 77", "wget": "exit 77",
            }.items():
                executable = binaries / binary
                executable.write_text(f"#!/bin/sh\n{content}\n")
                executable.chmod(0o700)
            environment = {key: value for key, value in os.environ.items()
                           if key not in {"XDG_CONFIG_HOME", "AETHER_BASE_URL", "AETHER_API_KEY"}}
            environment.update({"HOME": str(home), "PATH": f"{binaries}:{os.environ['PATH']}",
                                "NO_PROXY": "127.0.0.1", "no_proxy": "127.0.0.1"})
            config = home / ".config/opencode/opencode.json"
            config.parent.mkdir(parents=True)
            original = {
                "model": "keep/model", "small_model": "aether/fixture", "permission": {"bash": "ask"},
                "provider": {
                    "keep": {"options": {"apiKey": "another-synthetic-key",
                                         "headers": {"Authorization": "Bearer keep-provider-key"}}},
                    "aether": {"options": {"timeout": 90000, "headers": {
                        "aUtHoRiZaTiOn": "Bearer " + OLD_KEY, "X-aPi-KeY": OLD_KEY,
                        "pRoXy-AuThOrIzAtIoN": OLD_KEY, "X-Trace-Id": "keep-options-trace"}},
                               "headers": {"AUTHORIZATION": "Bearer " + OLD_KEY,
                                           "x-API-key": OLD_KEY, "X-Trace-Root": "keep-root-trace"},
                               "whitelist": ["fixture"],
                               "models": {"removed": {"name": "Stale"}}},
                },
            }
            original_raw = json.dumps(original).encode()
            config.write_bytes(original_raw)
            script = render_script("opencode", "opencode", "opencode-ai", server.base)
            sync_script = home / ".aether/opencode-sync.py"
            credentials = home / ".aether/opencode.json"

            def run(install=False, succeeds=True, trace=False):
                command = ["sh", "-xv"] if trace else ["sh"]
                result = subprocess.run(command if install else [sys.executable, str(sync_script)],
                                        input=script if install else None, text=True,
                                        capture_output=True, env=environment, timeout=15)
                assert (result.returncode == 0) == succeeds, result.stdout + result.stderr
                assert all(secret not in result.stdout + result.stderr for secret in (KEY, OLD_KEY, ROTATED_KEY))
                assert "Traceback" not in result.stderr
                return result

            run(install=True, trace=True)
            first = config.read_bytes()
            installed = json.loads(first)
            assert installed["model"] == original["model"]
            assert installed["small_model"] == original["small_model"]
            assert installed["permission"] == original["permission"]
            assert installed["provider"]["keep"] == original["provider"]["keep"]
            aether = installed["provider"]["aether"]
            assert aether["models"] == remote_provider["models"]
            assert aether["options"] == {"timeout": 90000, "baseURL": server.base + "/v1", "apiKey": KEY,
                                         "headers": {"X-Trace-Id": "keep-options-trace"}}
            assert aether["headers"] == {"X-Trace-Root": "keep-root-trace"}
            assert aether["whitelist"] == ["fixture"]
            assert KEY not in sync_script.read_text()
            assert json.loads(credentials.read_text())["api_key"] == KEY
            backups = list(config.parent.glob("opencode.json.bak.*"))
            assert len(backups) == 1 and backups[0].read_bytes() == original_raw
            for path in [config, credentials, *backups]:
                assert path.stat().st_mode & 0o777 == 0o600
            assert sync_script.stat().st_mode & 0o777 == 0o700
            assert credentials.parent.stat().st_mode & 0o777 == 0o700
            run(install=True)
            run()
            assert config.read_bytes() == first
            assert len(list(config.parent.glob("opencode.json.bak.*"))) == 1
            private_state = (credentials.read_bytes(), sync_script.read_bytes())

            # The same portable helper is used by macOS; reject a wrong selected OS.
            script = render_script("opencode", "opencode", "opencode-ai", server.base, system="windows")
            run(install=True, succeeds=False)
            assert config.read_bytes() == first
            (binaries / "uname").write_text("#!/bin/sh\nprintf 'Darwin\\n'\n")
            script = render_script("opencode", "opencode", "opencode-ai", server.base, system="macos")
            run(install=True)
            assert config.read_bytes() == first
            (binaries / "uname").write_text("#!/bin/sh\nprintf 'Linux\\n'\n")
            script = render_script("opencode", "opencode", "opencode-ai", server.base)

            # Both an installation retry and later sync must leave invalid input untouched.
            for raw in [b'{"broken":', b'{"provider": []}', b'{"provider":{"aether":[]}}',
                        b'{"provider":{"aether":{"options":null}}}', b'[]',
                        b'{"provider":{"aether":{"headers":null}}}',
                        b'{"provider":{"aether":{"headers":[]}}}',
                        b'{"provider":{"aether":{"options":{"headers":[]}}}}',
                        b'{"provider":{"aether":{"options":{"headers":"invalid"}}}}',
                        b'{"a":1,"a":2}', b'{"a":NaN}', b'{/* comment */"a":1}']:
                config.write_bytes(raw)
                for installing in [False, True]:
                    run(install=installing, succeeds=False)
                    assert config.read_bytes() == raw
                    assert (credentials.read_bytes(), sync_script.read_bytes()) == private_state
            config.write_bytes(first)
            jsonc = config.with_suffix(".jsonc")
            jsonc.write_text('{/* user comments */ "model":"keep/model"}')
            run(succeeds=False)
            run(install=True, succeeds=False)
            assert config.read_bytes() == first and "user comments" in jsonc.read_text()
            config.unlink()
            run(install=True, succeeds=False)
            assert not config.exists()
            jsonc.unlink()
            config.write_bytes(first)

            # Never overwrite on HTTP errors, unsafe redirects or malformed catalog data.
            valid_payload = server.payload
            for status, payload in [(401, KEY.encode()), (500, KEY.encode()),
                                    (302, b""), (200, b"broken"), (200, b"[]"),
                                    (200, b'{"provider":{"aether":{"models":{}}}}')]:
                server.status, server.payload = status, payload
                before_requests = len(server.requests)
                run(succeeds=False)
                assert len(server.requests) == before_requests + 1
                assert config.read_bytes() == first
                assert (credentials.read_bytes(), sync_script.read_bytes()) == private_state
            server.status, server.payload = 200, valid_payload
            for invalid_provider in [
                {**remote_provider, "headers": []},
                {**remote_provider, "options": {**remote_provider["options"], "headers": None}},
            ]:
                server.payload = json.dumps({"provider": {"aether": invalid_provider}}).encode()
                run(succeeds=False)
                assert config.read_bytes() == first
                assert (credentials.read_bytes(), sync_script.read_bytes()) == private_state
            server.payload = valid_payload

            # A failed first installation must not publish credentials or a helper.
            new_home = home / "first-install"
            new_home.mkdir()
            before_home = environment["HOME"]
            environment["HOME"] = str(new_home)
            server.status = 401
            run(install=True, succeeds=False)
            assert not (new_home / ".config/opencode/opencode.json").exists()
            assert not (new_home / ".aether/opencode.json").exists()
            assert not (new_home / ".aether/opencode-sync.py").exists()
            environment["HOME"] = before_home
            server.status = 200
            unsafe_provider = {**remote_provider, "options": {"baseURL": "https://untrusted.invalid/v1"}}
            server.payload = json.dumps({"provider": {"aether": unsafe_provider}}).encode()
            run(succeeds=False)
            assert config.read_bytes() == first
            server.payload = valid_payload

            credentials.chmod(0o644)
            run(succeeds=False)
            assert config.read_bytes() == first
            credentials.chmod(0o600)
            config.unlink()
            external = home / "do-not-write.json"
            external.write_bytes(first)
            config.symlink_to(external)
            run(succeeds=False)
            assert external.read_bytes() == first
            config.unlink()
            config.write_bytes(first)

            config.unlink()
            config.mkdir()
            run(succeeds=False)
            assert config.is_dir()
            config.rmdir()
            os.mkfifo(config, mode=0o600)
            run(succeeds=False)
            config.unlink()
            config.write_bytes(first)

            # A smaller effective catalog removes revoked models and retains full new meta.
            updated_provider = {**remote_provider, "models": {"new": {"name": "New", "tool_call": False}}}
            server.payload = json.dumps({"provider": {"aether": updated_provider}}).encode()
            result = run()
            updated = json.loads(config.read_text())
            assert updated["provider"]["aether"]["models"] == updated_provider["models"]
            assert updated["model"] == "keep/model" and "small_model" not in updated
            assert "Cleared unavailable Aether defaults: small_model" in result.stdout
            assert "aether/fixture" not in result.stdout
            assert len(list(config.parent.glob("opencode.json.bak.*"))) == 2

            # Clear only unavailable Aether references; retain valid Aether and all other providers.
            for model, small_model, expected_model, expected_small in [
                ("aether/removed", "aether/fixture", None, None),
                ("aether/new", "keep/small", "aether/new", "keep/small"),
                ("keep/model", "aether/new", "keep/model", "aether/new"),
                ("aether/removed", "keep/small", None, "keep/small"),
            ]:
                current = json.loads(config.read_text())
                current.update({"model": model, "small_model": small_model})
                config.write_text(json.dumps(current))
                result = run()
                current = json.loads(config.read_text())
                for field, expected in [("model", expected_model), ("small_model", expected_small)]:
                    if expected is None:
                        assert field not in current
                    else:
                        assert current[field] == expected
                assert current["provider"]["keep"] == original["provider"]["keep"]
                assert ("Cleared unavailable Aether defaults:" in result.stdout) == (expected_model is None)

            # A fresh selected/rotated key replaces stale mixed-case credentials, not trace headers.
            current = json.loads(config.read_text())
            current["provider"]["aether"]["options"]["headers"].update({
                "aUtHoRiZaTiOn": "Bearer " + KEY, "X-aPi-KeY": KEY,
                "X-Goog-Api-Key": KEY, "X-AuTh-ToKeN": KEY,
            })
            current["provider"]["aether"]["headers"].update({
                "AUTHORIZATION": "Bearer " + KEY, "Api-Key": KEY, "X-Access-Token": KEY,
            })
            config.write_text(json.dumps(current))
            rotation_start = len(server.requests)
            script = render_script("opencode", "opencode", "opencode-ai", server.base, api_key=ROTATED_KEY)
            result = run(install=True, trace=True)
            rotated = json.loads(config.read_text())["provider"]["aether"]
            assert rotated["options"]["apiKey"] == ROTATED_KEY
            assert rotated["options"]["headers"] == {"X-Trace-Id": "keep-options-trace"}
            assert rotated["headers"] == {"X-Trace-Root": "keep-root-trace"}
            assert json.loads(credentials.read_text())["api_key"] == ROTATED_KEY
            unchanged = config.read_bytes()
            backup_count = len(list(config.parent.glob("opencode.json.bak.*")))
            result = run()
            assert config.read_bytes() == unchanged
            assert len(list(config.parent.glob("opencode.json.bak.*"))) == backup_count
            assert "Cleared unavailable Aether defaults:" not in result.stdout
            for path in config.parent.glob("opencode.json.bak.*"):
                assert path.stat().st_mode & 0o777 == 0o600
            assert all(path == "/api/opencode/config" and auth == "Bearer " + (KEY if index < rotation_start else ROTATED_KEY)
                       for index, (path, auth) in enumerate(server.requests))

            # First-time setup can honor XDG, and later sync uses that persisted destination.
            environment["XDG_CONFIG_HOME"] = str(home / "custom-config")
            run(install=True)
            xdg_config = home / "custom-config/opencode/opencode.json"
            assert xdg_config.exists()
            environment.pop("XDG_CONFIG_HOME")
            run()
            assert json.loads(credentials.read_text())["config_path"] == str(xdg_config)
            server.shutdown()
            server.server_close()
            before = xdg_config.read_bytes()
            run(succeeds=False)
            assert xdg_config.read_bytes() == before
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def main():
    test_existing_clients()
    test_opencode()
    print("PASS: Codex/Claude/OpenCode setup, repeatability, merge/backup, private permissions, safe failures and persistent sync")


if __name__ == "__main__":
    main()
