#!/usr/bin/env python3
"""
Pangu CLI - Command line interface for Pangu Gateway

Usage:
    pangu setup                    Configure gateway connection
    pangu run "task"              Run an agent task
    pangu sessions                List all sessions
    pangu session <id>            Show session details
    pangu config                  Show current config
    pangu tools                   List available tools
    pangu health                  Health check

Config: ~/.pangu/config.json
Environment overrides:
    PANGU_BASE_URL    Gateway URL
    PANGU_API_KEY     LLM API key
"""

import argparse
import json
import os
import sys
import urllib.request
import urllib.error

CONFIG_DIR = os.path.expanduser("~/.pangu")
CONFIG_FILE = os.path.join(CONFIG_DIR, "config.json")


def load_config() -> dict:
    if os.path.exists(CONFIG_FILE):
        with open(CONFIG_FILE) as f:
            return json.load(f)
    return {}


def save_config(cfg: dict):
    os.makedirs(CONFIG_DIR, exist_ok=True)
    with open(CONFIG_FILE, "w") as f:
        json.dump(cfg, f, indent=2)
    os.chmod(CONFIG_FILE, 0o600)


def get_base_url() -> str:
    return os.environ.get("PANGU_BASE_URL") or load_config().get("base_url", "http://localhost:4848")


def api(path: str, method: str = "GET", data: dict = None) -> dict:
    url = f"{get_base_url()}{path}"
    headers = {"Content-Type": "application/json", "Accept": "application/json"}

    body = json.dumps(data).encode() if data else None
    req = urllib.request.Request(url, data=body, headers=headers, method=method)

    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        try:
            err = json.loads(body)
            print(f"API error {e.code}: {err}", file=sys.stderr)
        except Exception:
            print(f"HTTP error {e.code}: {body[:200]}", file=sys.stderr)
        sys.exit(1)
    except urllib.error.URLError as e:
        print(f"Connection error: {e.reason}", file=sys.stderr)
        print(f"Is the gateway running at {get_base_url()}?", file=sys.stderr)
        sys.exit(1)


def input_default(prompt: str, default: str) -> str:
    val = input(f"{prompt} [{default}]: ").strip()
    return val if val else default


def cmd_setup():
    """Interactive configuration wizard."""
    print("=" * 50)
    print("  Pangu Gateway Setup")
    print("=" * 50)
    print()

    cfg = load_config()

    # 1. Gateway URL
    print("[1/5] Gateway URL")
    base_url = input_default(
        "  Base URL",
        cfg.get("base_url", "http://localhost:4848")
    )

    # 2. LLM Provider
    print()
    print("[2/5] LLM Provider")
    print("  1) OpenAI        (GPT-4o, GPT-4o-mini, etc.)")
    print("  2) Anthropic     (Claude 3.5 Sonnet, etc.)")
    print("  3) OpenAI-compatible (other providers)")
    provider_map = {"1": "openai", "2": "anthropic", "3": "openai-compatible"}
    provider_choice = input("  Choice [1]: ").strip() or "1"
    provider = provider_map.get(provider_choice, "openai")

    # 3. API Key
    print()
    print("[3/5] API Key")
    api_key = os.environ.get("PANGU_API_KEY") or cfg.get("api_key", "")
    if not api_key:
        api_key = input("  API Key: ").strip()
    if not api_key:
        print("  WARNING: No API key set. LLM calls will fail.")

    # 4. Model
    print()
    print("[4/5] Model")
    defaults = {
        "openai": "gpt-4o-mini",
        "anthropic": "claude-sonnet-4-20250514",
        "openai-compatible": "gpt-4o-mini",
    }
    model = input_default("  Model", cfg.get("model", defaults.get(provider, "gpt-4o-mini")))

    # 5. Base URL (for OpenAI-compatible)
    base_url_llm = None
    if provider == "openai-compatible":
        print()
        print("[5/5] OpenAI-compatible Base URL")
        base_url_llm = input_default(
            "  Base URL",
            cfg.get("base_url_llm", "https://api.openai.com/v1")
        )
    else:
        print()
        print("[5/5] (skip — default used)")

    # Build config
    new_cfg = {
        "base_url": base_url,
        "provider": provider,
        "api_key": api_key,
        "model": model,
    }
    if base_url_llm:
        new_cfg["base_url_llm"] = base_url_llm

    save_config(new_cfg)
    print()
    print("=" * 50)
    print("  Config saved to ~/.pangu/config.json")
    print("=" * 50)

    # Test connection
    print()
    print("Testing connection...")
    try:
        os.environ["PANGU_BASE_URL"] = base_url
        health = api("/health")
        print(f"  Gateway: {health.get('status', '?')}")
    except Exception as e:
        print(f"  Gateway connection failed: {e}")
        print("  (Gateway may not be running — start with: pangu-agent)")


def cmd_run(task: str, session_id: str = None, max_iterations: int = None):
    payload = {"task": task}
    if session_id:
        payload["session_id"] = session_id
    if max_iterations is not None:
        payload["max_iterations"] = max_iterations

    print(f"Running task...", file=sys.stderr)
    result = api("/v1/run", method="POST", data=payload)

    print(f"\n{'='*60}")
    print(f"Session: {result['session_id']}")
    print(f"Iterations: {result['iterations']}  Tool calls: {result['tool_calls']}")
    print(f"{'='*60}")
    print(f"\n{result['result']}")


def cmd_sessions():
    data = api("/v1/sessions")
    sessions = data if isinstance(data, list) else data.get("sessions", [data])
    if not sessions:
        print("No sessions found.")
        return
    print(f"{'Session ID':<40}  {'Messages':>8}")
    print("-" * 52)
    for s in sessions:
        sid = s.get("session_id", "?")
        msg_count = s.get("message_count", s.get("turn_count", "?"))
        print(f"{sid:<40}  {msg_count:>8}")


def cmd_session_detail(session_id: str):
    data = api(f"/v1/sessions/{session_id}")
    print(json.dumps(data, indent=2, ensure_ascii=False))


def cmd_config():
    cfg = load_config()
    if cfg:
        print(json.dumps(cfg, indent=2, ensure_ascii=False))
    else:
        print("No config file. Run 'pangu setup' first.")
        print(f"Default base_url: http://localhost:4848")


def cmd_tools():
    data = api("/v1/tools")
    tools = data if isinstance(data, list) else data.get("tools", [data])
    if not tools:
        print("No tools registered.")
        return
    for t in tools:
        name = t.get("name", "?")
        desc = t.get("description", "")
        print(f"  {name:<20}  {desc}")


def cmd_health():
    data = api("/health")
    print(f"Status: {data.get('status', data)}")


def cmd_env():
    """Show effective config (env + file merged)."""
    env_url = os.environ.get("PANGU_BASE_URL", "")
    env_key = os.environ.get("PANGU_API_KEY", "***")
    file_cfg = load_config()
    print(f"Base URL : {env_url or file_cfg.get('base_url', 'http://localhost:4848')} (env: {'set' if env_url else 'not set'})")
    print(f"API Key  : {env_key if env_key != '***' else file_cfg.get('api_key', '***')[:8] + '...' if file_cfg.get('api_key') else 'not set'}")
    print(f"Provider : {file_cfg.get('provider', 'not set')}")
    print(f"Model    : {file_cfg.get('model', 'not set')}")


def main():
    parser = argparse.ArgumentParser(description="Pangu CLI", prog="pangu")
    sub = parser.add_subparsers(dest="cmd", required=True)

    sub.add_parser("setup", help="Configure gateway connection (interactive)")

    r = sub.add_parser("run", help="Run an agent task")
    r.add_argument("task", help="Task description")
    r.add_argument("-s", "--session", dest="session_id", help="Session ID to continue")
    r.add_argument("-i", "--iterations", type=int, dest="max_iterations", help="Max iterations")

    sub.add_parser("sessions", help="List all sessions")

    s = sub.add_parser("session", help="Show session details")
    s.add_argument("session_id", help="Session ID")

    sub.add_parser("config", help="Show local config file")
    sub.add_parser("env", help="Show effective config (env + file)")
    sub.add_parser("tools", help="List available tools")
    sub.add_parser("health", help="Health check")

    args = parser.parse_args()

    if args.cmd == "setup":
        cmd_setup()
    elif args.cmd == "run":
        cmd_run(args.task, args.session_id, args.max_iterations)
    elif args.cmd == "sessions":
        cmd_sessions()
    elif args.cmd == "session":
        cmd_session_detail(args.session_id)
    elif args.cmd == "config":
        cmd_config()
    elif args.cmd == "env":
        cmd_env()
    elif args.cmd == "tools":
        cmd_tools()
    elif args.cmd == "health":
        cmd_health()


if __name__ == "__main__":
    main()
