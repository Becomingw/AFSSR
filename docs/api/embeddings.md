# Embeddings API

Arca supports OpenAI compatible embedding requests through `POST /v1/embeddings`. Embedding requests are separate from chat and responses requests. They use `input`, never `messages`, and they are always non streaming.

## Quick Start

Run this against your Arca gateway URL with a user API key that can access the model and the `openai:embedding` API format.

```bash
curl -sS "http://localhost:8084/v1/embeddings" \
  -H "Authorization: Bearer sk-your-aether-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "text-embedding-3-small",
    "input": ["hello", "world"],
    "encoding_format": "float"
  }'
```

## Public Endpoint

| Method | Path | Client API format | Route kind |
| --- | --- | --- | --- |
| `POST` | `/v1/embeddings` | `openai:embedding` | `embedding` |

The gateway classifies this endpoint as an OpenAI family embedding route with endpoint signature `openai:embedding`. It is not handled as chat or responses.

## Request Body

Required fields:

| Field | Type | Notes |
| --- | --- | --- |
| `model` | string | Must name a model allowed for the API key and user. Blank strings are rejected. |
| `input` | string, string array, integer token array, nested integer token arrays, or multimodal object array | Must be non empty. Empty strings, empty arrays, empty token arrays, and empty multimodal objects are rejected. |

Optional fields that pass through the embedding conversion path when supported by the provider:

| Field | Notes |
| --- | --- |
| `encoding_format` | Passed to OpenAI compatible providers. |
| `dimensions` | Passed to providers whose embedding request shape supports it. |
| `parameters` | Provider-specific embedding parameters. For Aliyun DashScope this maps to DashScope `parameters`; `dimensions` is emitted as `parameters.dimension` unless `parameters.dimension` is already set. |
| `user` | Passed to OpenAI compatible providers. |
| `task` | Passed to Jina and OpenAI compatible embedding requests. Jina defaults to `text-matching` when no task is supplied. |

Accepted `input` shapes:

```json
{ "model": "text-embedding-3-small", "input": "hello" }
```

```json
{ "model": "text-embedding-3-small", "input": ["hello", "world"] }
```

```json
{ "model": "text-embedding-3-small", "input": [1, 2, 3] }
```

```json
{ "model": "text-embedding-3-small", "input": [[1, 2], [3, 4]] }
```

```json
{
  "model": "qwen3-vl-embedding",
  "input": [
    { "text": "white running shoes" },
    { "image": "https://dashscope.oss-cn-beijing.aliyuncs.com/images/256_1.png" }
  ],
  "parameters": { "enable_fusion": true }
}
```

Use string or string array input when routing to Gemini or Doubao embedding providers. Token arrays are accepted by the OpenAI compatible public endpoint, but Gemini, Doubao, and Aliyun provider request emitters require text or multimodal content input.

## Provider Format Mapping

Embedding routes can select only embedding provider API formats. Chat, responses, image, and generation formats are not valid provider targets for this request type.

| Provider API format | Upstream path shape | Provider request shape |
| --- | --- | --- |
| `openai:embedding` | `/v1/embeddings` | OpenAI compatible `{ "model", "input" }` payload. |
| `jina:embedding` | `/v1/embeddings` | OpenAI compatible payload with a Jina `task`. Defaults to `text-matching` if omitted. |
| `gemini:embedding` | `models/{model}:embedContent` | Single text input uses `content.parts[].text`. Multiple text inputs use `requests[].content.parts[].text`. |
| `doubao:embedding` | `/embeddings/multimodal` | Text input is emitted as `input` items like `{ "type": "text", "text": "..." }`. |
| `aliyun:multimodal_embedding` | `/api/v1/services/embeddings/multimodal-embedding/multimodal-embedding` | Text and multimodal inputs are emitted as DashScope `input.contents`. Supports `text`, `image`, `video`, `multi_images`, `parameters.enable_fusion`, `parameters.res_level`, and `parameters.max_video_frames`. Alias: `dashscope:multimodal_embedding`. |

Custom provider endpoint paths are available when the endpoint is configured for an embedding API format. Gemini custom paths can use `{model}` and `{action}`. For `gemini:embedding`, `{action}` expands to `embedContent`.

## Model And Catalog Requirements

To use embeddings through the gateway:

1. The global model should include embedding metadata, for example `supported_capabilities: ["embedding"]`, `config.model_type: "embedding"`, or `config.api_formats` with one of the embedding formats.
2. The provider model or mapping must expose an embedding API format, one of `openai:embedding`, `gemini:embedding`, `jina:embedding`, `doubao:embedding`, or `aliyun:multimodal_embedding`.
3. The user and API key must be allowed to access the model and the `openai:embedding` client API format.
4. Public and admin catalog responses expose `supports_embedding` so clients can display embedding capability separately from chat.

Billing fails closed for embedding global models. A model marked as embedding capable must define either `default_price_per_request` or `default_tiered_pricing.tiers[].input_price_per_1m`. Missing request pricing and missing input token pricing cause the model record to be rejected instead of treated as free.

No schema migration is needed for embedding metadata. Existing model capability, config, provider mapping, API format, and pricing fields carry the data.

## Aliyun Qwen3-VL Examples

Text request through Arca:

```bash
curl -sS "http://localhost:8084/v1/embeddings" \
  -H "Authorization: Bearer sk-your-aether-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-vl-embedding",
    "input": "white running shoes",
    "dimensions": 1024
  }'
```

Image and text fusion request:

```bash
curl -sS "http://localhost:8084/v1/embeddings" \
  -H "Authorization: Bearer sk-your-aether-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-vl-embedding",
    "input": [
      { "text": "white running shoes, lightweight and breathable" },
      { "image": "https://dashscope.oss-cn-beijing.aliyuncs.com/images/256_1.png" }
    ],
    "parameters": { "enable_fusion": true }
  }'
```

Video request:

```bash
curl -sS "http://localhost:8084/v1/embeddings" \
  -H "Authorization: Bearer sk-your-aether-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-vl-embedding",
    "input": [
      { "video": "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20250107/lbcemt/new+video.mp4" }
    ],
    "parameters": { "max_video_frames": 64 }
  }'
```

Multi-image fusion request:

```bash
curl -sS "http://localhost:8084/v1/embeddings" \
  -H "Authorization: Bearer sk-your-aether-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-vl-embedding",
    "input": [
      { "text": "product photos from multiple angles" },
      { "multi_images": [
        "https://example.com/front.png",
        "https://example.com/side.png"
      ] }
    ],
    "parameters": { "enable_fusion": true }
  }'
```

## Failure Behavior

The gateway validates deterministic request errors before local execution or provider transport.

| Case | Example request body or setup | Status | Error detail |
| --- | --- | --- | --- |
| Invalid JSON | `{` | `400` | `Embedding request JSON body is invalid` |
| Missing model | `{ "input": "hello" }` | `400` | `Embedding request model is required` |
| Empty input | `{ "model": "text-embedding-3-small", "input": [] }` | `400` | `Embedding request input is required` |
| Chat `messages` payload | `{ "model": "text-embedding-3-small", "messages": [] }` | `400` | `Embedding request must use input, not chat messages` |
| Streaming requested | `{ "model": "text-embedding-3-small", "input": "hello", "stream": true }` | `400` | `Embedding requests do not support streaming` |
| Non JSON content type | `Content-Type: text/plain` with an embedding JSON body | `400` | `Embedding request content-type must be application/json` |
| Chat only model | API key allows `text-embedding-3-small`, request uses `gpt-5` | `403` | The key is not allowed to access that model. |
| Chat only API format | API key allows `openai:chat` but not `openai:embedding` | `403` | The key is not allowed to access `openai:embedding`. |

Failure examples:

```bash
curl -sS "http://localhost:8084/v1/embeddings" \
  -H "Authorization: Bearer sk-your-aether-key" \
  -H "Content-Type: application/json" \
  -d '{"model":"text-embedding-3-small","messages":[]}'
```

```bash
curl -sS "http://localhost:8084/v1/embeddings" \
  -H "Authorization: Bearer sk-your-aether-key" \
  -H "Content-Type: application/json" \
  -d '{"input":"hello"}'
```

```bash
curl -sS "http://localhost:8084/v1/embeddings" \
  -H "Authorization: Bearer sk-your-aether-key" \
  -H "Content-Type: application/json" \
  -d '{"model":"text-embedding-3-small","input":[]}'
```

```bash
curl -sS "http://localhost:8084/v1/embeddings" \
  -H "Authorization: Bearer sk-your-aether-key" \
  -H "Content-Type: application/json" \
  -d '{"model":"text-embedding-3-small","input":"hello","stream":true}'
```

```bash
curl -sS "http://localhost:8084/v1/embeddings" \
  -H "Authorization: Bearer sk-your-aether-key" \
  -H "Content-Type: text/plain" \
  -d '{"model":"text-embedding-3-small","input":"hello"}'
```

If a valid embedding request passes local validation but no usable provider transport is available, the gateway can return a provider or service availability error. That is different from the deterministic request validation errors above.
