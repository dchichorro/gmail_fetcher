#!/usr/bin/env python3
"""One-shot OAuth2 helper for gmail_fetcher.
Run this locally, visit the URL, authorize, and it saves token.json.
Then copy token.json to devbox.

Requires CLIENT_ID and CLIENT_SECRET environment variables (or .env file)."""

import json
import os
import sys
import time
import urllib.parse
from http.server import HTTPServer, BaseHTTPRequestHandler
from threading import Thread

try:
    import requests
except ImportError:
    print("Need requests: pip install requests")
    sys.exit(1)

# Load .env if present
def load_dotenv():
    env_path = os.path.join(os.path.dirname(__file__), ".env")
    if os.path.exists(env_path):
        with open(env_path) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and "=" in line:
                    key, _, value = line.partition("=")
                    os.environ.setdefault(key.strip(), value.strip())

load_dotenv()

CLIENT_ID = os.environ.get("CLIENT_ID", "")
CLIENT_SECRET = os.environ.get("CLIENT_SECRET", "")
if not CLIENT_ID or not CLIENT_SECRET:
    print("Error: CLIENT_ID and CLIENT_SECRET must be set (in .env or environment)")
    sys.exit(1)
REDIRECT_URI = "http://localhost:8080/callback"
SCOPE = "https://www.googleapis.com/auth/gmail.readonly"
AUTH_URL = "https://accounts.google.com/o/oauth2/auth"
TOKEN_URL = "https://oauth2.googleapis.com/token"

auth_code = None

class CallbackHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        global auth_code
        parsed = urllib.parse.urlparse(self.path)
        params = urllib.parse.parse_qs(parsed.query)
        if "code" in params:
            auth_code = params["code"][0]
            self.send_response(200)
            self.send_header("Content-Type", "text/html")
            self.end_headers()
            self.wfile.write(b"<h1>Authorization successful!</h1><p>You can close this tab.</p>")
        else:
            self.send_response(400)
            self.send_header("Content-Type", "text/html")
            self.end_headers()
            error = params.get("error", ["unknown"])[0]
            self.wfile.write(f"<h1>Authorization failed: {error}</h1>".encode())
    def log_message(self, format, *args):
        pass

# Build auth URL
auth_params = urllib.parse.urlencode({
    "client_id": CLIENT_ID,
    "redirect_uri": REDIRECT_URI,
    "response_type": "code",
    "scope": SCOPE,
    "access_type": "offline",
    "prompt": "consent",
})
url = f"{AUTH_URL}?{auth_params}"

print(f"\n{'='*50}")
print(f"Open this URL in your browser:\n\n{url}\n")
print(f"After authorizing, you'll be redirected to localhost:8080")
print(f"{'='*50}\n")

# Start local server
server = HTTPServer(("localhost", 8080), CallbackHandler)
thread = Thread(target=server.handle_request, daemon=True)
thread.start()

print("Waiting for authorization... (Ctrl+C to cancel)")
thread.join(timeout=120)
server.server_close()

if not auth_code:
    print("No authorization code received. Timed out or cancelled.")
    sys.exit(1)

print(f"\nGot authorization code: {auth_code[:20]}...")

# Exchange code for tokens
print("Exchanging code for tokens...")
resp = requests.post(TOKEN_URL, data={
    "code": auth_code,
    "client_id": CLIENT_ID,
    "client_secret": CLIENT_SECRET,
    "redirect_uri": REDIRECT_URI,
    "grant_type": "authorization_code",
})

if resp.status_code != 200:
    print(f"Token exchange failed: {resp.status_code} {resp.text}")
    sys.exit(1)

token_data = resp.json()
import time as _time
token = {
    "access_token": token_data["access_token"],
    "refresh_token": token_data["refresh_token"],
    "expires_at": int(_time.time()) + token_data.get("expires_in", 3600),
}

with open("token.json", "w") as f:
    json.dump(token, f, indent=2)

print(f"\nDone! token.json saved.")
print(f"Copy it to devbox: scp token.json dchichorro@100.115.122.126:~/gmail_fetcher/")
