"""Reproduce ffprobe's metadata-exit behavior on an intentionally partial 206 body.

This is evidence tooling only. It does not start Chronobreak or change benchmark
behavior. The server sends the first 200,704 bytes of a 296,175-byte clip and
    keeps the connection open; a client close/reset is the observable server-side
    cancellation. The child result is captured directly.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import socket
import subprocess
import threading


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("media", type=pathlib.Path)
    parser.add_argument("ffprobe", type=pathlib.Path)
    args = parser.parse_args()
    media = args.media.read_bytes()
    total = len(media)
    sent = min(200_704, total)

    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    port = listener.getsockname()[1]
    observed: dict[str, object] = {}

    def serve() -> None:
        connection, _ = listener.accept()
        try:
            connection.settimeout(12)
            request = connection.recv(4096)
            observed["request"] = request.decode("latin1", "replace").split(
                "\r\n", 1
            )[0]
            headers = (
                "HTTP/1.1 206 Partial Content\r\n"
                "Content-Type: video/mp4\r\n"
                f"Content-Length: {total}\r\n"
                f"Content-Range: bytes 0-{total - 1}/{total}\r\n"
                "Connection: keep-alive\r\n\r\n"
            ).encode("ascii")
            connection.sendall(headers)
            connection.sendall(media[:sent])
            observed["declared_bytes"] = total
            observed["delivered_bytes"] = sent
            observed["client_closed"] = connection.recv(1) == b""
        except Exception as error:  # Windows reports a reset when the client closes.
            observed["server_error"] = type(error).__name__
            observed["server_error_text"] = str(error)
        finally:
            connection.close()
            listener.close()

    thread = threading.Thread(target=serve, daemon=True)
    thread.start()
    url = f"http://127.0.0.1:{port}/clips/{args.media.name}"
    child = subprocess.run(
        [
            str(args.ffprobe),
            "-v",
            "error",
            "-f",
            "mov",
            "-enable_drefs",
            "0",
            "-use_absolute_path",
            "0",
            "-protocol_whitelist",
            "http,tcp",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            url,
        ],
        capture_output=True,
        text=True,
        timeout=12,
    )
    observed.update(
        {
            "child_returncode": child.returncode,
            "child_stdout": child.stdout,
            "child_stderr": child.stderr,
            "url_shape": "/clips/<id>.mp4",
        }
    )
    thread.join(timeout=2)
    print(json.dumps(observed, sort_keys=True))


if __name__ == "__main__":
    main()
