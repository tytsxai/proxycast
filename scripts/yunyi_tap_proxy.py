#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""
一个用于抓包/对齐请求的“透明转发代理”：
- 在本地以 HTTP 方式监听
- 记录收到的 headers/body（用于对齐 codex-cli 的真实请求）
- 原样转发到上游 HTTPS，并把响应（含 SSE）流式回传给客户端

用法示例：
  1) 启动代理（监听 18080，转发到 https://yunyi.cfd）：
     python3 scripts/yunyi_tap_proxy.py --port 18080 --upstream https://yunyi.cfd

  2) 让 codex-cli 指向本地代理：
     codex exec "只回复一个字：好" --json -C . \\
       -c 'model_provider=\"yunyi\"' \\
       -c 'model_providers.yunyi.base_url=\"http://127.0.0.1:18080/codex\"'
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from http.client import HTTPSConnection
from urllib.parse import urlparse


SENSITIVE_HEADERS = {
    "authorization",
    "cookie",
    "set-cookie",
}


def _redact_header(name: str, value: str) -> str:
    if name.lower() in SENSITIVE_HEADERS:
        return "[REDACTED]"
    return value


def _try_pretty_json(raw: bytes) -> str:
    try:
        obj = json.loads(raw.decode("utf-8"))
    except Exception:
        return raw.decode("utf-8", "replace")
    return json.dumps(obj, ensure_ascii=False, indent=2)


class TapProxyHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    upstream_scheme: str = "https"
    upstream_host: str = ""
    upstream_port: int = 443

    def log_message(self, fmt: str, *args):
        # 统一输出到 stderr，避免污染转发响应
        sys.stderr.write("%s - - [%s] %s\n" % (self.client_address[0], self.log_date_time_string(), fmt % args))

    def _read_body(self) -> bytes:
        length = self.headers.get("Content-Length")
        if not length:
            return b""
        try:
            n = int(length)
        except ValueError:
            return b""
        return self.rfile.read(n)

    def _forward(self):
        body = self._read_body()

        # 打印抓到的请求（脱敏）
        sys.stderr.write("\n===== TAP 请求开始 =====\n")
        sys.stderr.write(f"{self.command} {self.path}\n")
        for k, v in self.headers.items():
            sys.stderr.write(f"{k}: {_redact_header(k, v)}\n")
        if body:
            sys.stderr.write("\n--- body ---\n")
            sys.stderr.write(_try_pretty_json(body))
            sys.stderr.write("\n")
        else:
            sys.stderr.write("\n(无 body)\n")
        sys.stderr.write("===== TAP 请求结束 =====\n\n")

        conn = HTTPSConnection(self.upstream_host, self.upstream_port, timeout=120)

        # 复制 headers：移除 hop-by-hop，更新 Host
        hop_by_hop = {
            "connection",
            "keep-alive",
            "proxy-authenticate",
            "proxy-authorization",
            "te",
            "trailers",
            "transfer-encoding",
            "upgrade",
        }
        forward_headers = {}
        for k, v in self.headers.items():
            lk = k.lower()
            if lk in hop_by_hop:
                continue
            if lk == "host":
                continue
            forward_headers[k] = v
        forward_headers["Host"] = self.upstream_host

        conn.request(self.command, self.path, body=body if body else None, headers=forward_headers)
        resp = conn.getresponse()

        # 回写响应 headers/status
        self.send_response(resp.status, resp.reason)
        resp_headers = resp.getheaders()
        for k, v in resp_headers:
            # BaseHTTPRequestHandler 会自动处理分块/连接；这里尽量原样转发
            if k.lower() == "transfer-encoding":
                continue
            self.send_header(k, v)
        self.end_headers()

        # 流式转发响应体（SSE/普通 JSON 都可）
        while True:
            chunk = resp.read(8192)
            if not chunk:
                break
            self.wfile.write(chunk)
            try:
                self.wfile.flush()
            except BrokenPipeError:
                break

        conn.close()

    def do_POST(self):
        self._forward()

    def do_GET(self):
        self._forward()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=18080)
    ap.add_argument("--upstream", type=str, required=True, help="例如：https://yunyi.cfd")
    args = ap.parse_args()

    u = urlparse(args.upstream)
    if u.scheme != "https":
        raise SystemExit("upstream 仅支持 https")
    host = u.hostname
    port = u.port or 443
    if not host:
        raise SystemExit("upstream 解析失败")

    TapProxyHandler.upstream_host = host
    TapProxyHandler.upstream_port = port

    httpd = ThreadingHTTPServer(("127.0.0.1", args.port), TapProxyHandler)
    sys.stderr.write(f"Tap proxy listening on http://127.0.0.1:{args.port} (-> https://{host}:{port})\n")
    httpd.serve_forever()


if __name__ == "__main__":
    main()

