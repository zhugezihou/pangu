#!/usr/bin/env python3
"""
Pangu Chat - Interactive TUI for Pangu Gateway

Usage:
    pangu chat              Start interactive chat
    pangu chat -s <id>     Continue session <id>
"""

import argparse
import json
import os
import sys
import urllib.request
import urllib.error
import time

CONFIG_FILE = os.path.expanduser("~/.pangu/config.json")


def load_config() -> dict:
    if os.path.exists(CONFIG_FILE):
        with open(CONFIG_FILE) as f:
            return json.load(f)
    return {}


def get_base_url() -> str:
    return os.environ.get("PANGU_BASE_URL") or load_config().get("base_url", "http://localhost:4848")


def api(path: str, method: str = "GET", data: dict = None) -> dict:
    url = f"{get_base_url()}{path}"
    headers = {"Content-Type": "application/json", "Accept": "application/json"}
    body = json.dumps(data).encode() if data else None
    req = urllib.request.Request(url, data=body, headers=headers, method=method)

    try:
        with urllib.request.urlopen(req, timeout=120) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        try:
            err = json.loads(body)
            print(f"\n[API error {e.code}] {err}")
        except Exception:
            print(f"\n[HTTP error {e.code}] {body[:200]}")
        return None
    except urllib.error.URLError as e:
        print(f"\n[Connection error] {e.reason}")
        print(f"Is the gateway running at {get_base_url()}?")
        return None


def cmd_chat(session_id: str = None, max_iterations: int = None):
    cfg = load_config()
    if not cfg.get("api_key"):
        print("[Config] No API key found. Run 'pangu setup' first.")
        print(f"  Config file: {CONFIG_FILE}")
        return

    # Banner
    print()
    print("=" * 60)
    print("  Pangu Chat  (type 'quit' to exit, 'help' for commands)")
    print("=" * 60)
    if session_id:
        print(f"  Resuming session: {session_id}")
    print(f"  Gateway: {get_base_url()}")
    print(f"  Model: {cfg.get('model', '?')}")
    print("=" * 60)
    print()

    current_session = session_id
    history = []  # (role, text)

    while True:
        try:
            user_input = input("\033[1;36m👤\033[0m ").strip()
        except (EOFError, KeyboardInterrupt):
            print("\n\nGoodbye!")
            break

        if not user_input:
            continue

        if user_input.lower() in ("quit", "exit", "q"):
            print("Goodbye!")
            break

        if user_input.lower() == "help":
            print("  quit / exit / q   - Exit")
            print("  help              - This message")
            print("  clear             - Clear screen")
            print("  session           - Show current session ID")
            print("  sessions          - List all sessions")
            print("  rerun <text>      - Re-run last user message")
            continue

        if user_input.lower() == "clear":
            print("\033[2J\033[H", end="")
            continue

        if user_input.lower() == "session":
            if current_session:
                print(f"  Current session: {current_session}")
            else:
                print("  No active session")
            continue

        if user_input.lower() == "sessions":
            data = api("/v1/sessions")
            if data:
                sessions = data if isinstance(data, list) else data.get("sessions", [])
                if sessions:
                    for s in sessions:
                        print(f"  {s.get('session_id', '?')}")
                else:
                    print("  No sessions")
            continue

        # Build request
        payload = {"task": user_input}
        if current_session:
            payload["session_id"] = current_session
        if max_iterations:
            payload["max_iterations"] = max_iterations

        print("\033[1;90m    Running...\033[0m", end="\r", flush=True)

        start = time.time()
        result = api("/v1/run", method="POST", data=payload)
        elapsed = time.time() - start

        if result is None:
            continue

        current_session = result.get("session_id", current_session)

        # Print response
        print(f"\033[1;32m🤖\033[0m [{result.get('iterations', '?')} iter, "
              f"{result.get('tool_calls', '?')} tools, {elapsed:.1f}s]")
        print("-" * 60)
        print(result.get("result", "(no result)"))
        print()

        history.append(("user", user_input))
        history.append(("assistant", result.get("result", "")))

        # Auto-save session info
        print(f"\033[1;90m    Session: {current_session}\033[0m", end="  \r")


def main():
    parser = argparse.ArgumentParser(description="Pangu Chat", prog="pangu chat")
    parser.add_argument("-s", "--session", dest="session_id", help="Session ID to continue")
    parser.add_argument("-i", "--iterations", type=int, dest="max_iterations", help="Max iterations")
    args = parser.parse_args()

    cmd_chat(args.session_id, args.max_iterations)


if __name__ == "__main__":
    main()
