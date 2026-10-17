# Rerank API

Arca exposes an OpenAI-compatible rerank surface at `POST /v1/rerank` and can route it to providers configured as `openai:rerank` or `jina:rerank`.

## Request

```http
POST /v1/rerank
Authorization: Bearer <aether-api-key>
Content-Type: application/json
```

```json
{
  "model": "bge-reranker-base",
  "query": "What document discusses gateway routing?",
  "documents": [
    "Arca routes public AI requests through the Rust gateway.",
    "This document discusses unrelated content."
  ],
  "top_n": 1,
  "return_documents": true
}
```

Fields:

| Field | Required | Notes |
| --- | --- | --- |
| `model` | Yes | Arca global model name. |
| `query` | Yes | Non-empty query string. |
| `documents` | Yes | Non-empty array of strings or provider-native document objects. |
| `top_n` | No | Positive integer. |
| `return_documents` | No | Provider-compatible flag for including matched documents. |

## Response

Arca forwards the provider JSON response. OpenAI-compatible and Jina-compatible rerank providers commonly return `results[]`:

```json
{
  "model": "bge-reranker-base",
  "results": [
    {
      "index": 0,
      "relevance_score": 0.98,
      "document": {
        "text": "Arca routes public AI requests through the Rust gateway."
      }
    }
  ],
  "usage": {
    "total_tokens": 32
  }
}
```

Rerank requests must be JSON and do not support `stream` or chat `messages` payloads.
