#!/usr/bin/env python3
"""
embed_server.py — Serveur d'embeddings léger via llama-cpp-python.
Expose une API HTTP/JSON pour que ChronosAgent récupère de vrais embeddings.
"""
import sys
import json
import logging
from http.server import HTTPServer, BaseHTTPRequestHandler
from llama_cpp import Llama

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
log = logging.getLogger("embed_server")

MODEL_PATH = "/mnt/nvme/models/Qwen2.5-0.5B-Instruct.Q4_K_M.gguf"
PORT = 9097

class EmbedHandler(BaseHTTPRequestHandler):
    model = None

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length).decode("utf-8") if length > 0 else ""

        try:
            data = json.loads(body) if body else {}
            prompt = data.get("input", body.strip())
        except json.JSONDecodeError:
            prompt = body.strip()

        if not prompt:
            self._respond(400, {"error": "empty prompt"})
            return

        # Obtenir l'embedding
        embedding = EmbedHandler._get_embedding(prompt)
        if embedding is None:
            self._respond(500, {"error": "embedding failed"})
            return

        self._respond(200, {
            "dim": len(embedding),
            "embedding": embedding,
            "model": "qwen2.5-0.5b"
        })

    def do_GET(self):
        if self.path == "/health":
            self._respond(200, {"status": "ok", "model": MODEL_PATH})
        else:
            self._respond(404, {"error": "not found"})

    def _respond(self, code, data):
        body = json.dumps(data).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    @staticmethod
    def _get_embedding(text: str):
        if EmbedHandler.model is None:
            log.info("Loading model %s ...", MODEL_PATH)
            EmbedHandler.model = Llama(
                model_path=MODEL_PATH,
                embedding=True,
                n_ctx=2048,
                n_threads=32,
                n_gpu_layers=0,
                verbose=False,
            )
            log.info("Model loaded: dim=%d", EmbedHandler.model.embd_size())

        try:
            result = EmbedHandler.model.create_embedding(text)
            if "data" in result and len(result["data"]) > 0:
                return result["data"][0]["embedding"]
            return None
        except Exception as e:
            log.error("Embedding error: %s", e)
            return None

    def log_message(self, format, *args):
        log.info(format, *args)


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else PORT
    server = HTTPServer(("0.0.0.0", port), EmbedHandler)
    log.info("Embedding server listening on port %d", port)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        server.shutdown()


if __name__ == "__main__":
    main()
