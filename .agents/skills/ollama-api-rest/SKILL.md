> ## Documentation Index
> Fetch the complete documentation index at: https://docs.ollama.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Introduction

Ollama's API allows you to run and interact with models programatically.

## Get started

If you're just getting started, follow the [quickstart](/quickstart) documentation to get up and running with Ollama's API.

## Base URL

After installation, Ollama's API is served by default at:

```
http://localhost:11434/api
```

For running cloud models on **ollama.com**, the same API is available with the following base URL:

```
https://ollama.com/api
```

## Example request

Once Ollama is running, its API is automatically available and can be accessed via `curl`:

```shell  theme={"system"}
curl http://localhost:11434/api/generate -d '{
  "model": "gemma3",
  "prompt": "Why is the sky blue?"
}'
```

## Libraries

Ollama has official libraries for Python and JavaScript:

* [Python](https://github.com/ollama/ollama-python)
* [JavaScript](https://github.com/ollama/ollama-js)

Several community-maintained libraries are available for Ollama. For a full list, see the [Ollama GitHub repository](https://github.com/ollama/ollama?tab=readme-ov-file#libraries-1).

## Versioning

Ollama's API isn't strictly versioned, but the API is expected to be stable and backwards compatible. Deprecations are rare and will be announced in the [release notes](https://github.com/ollama/ollama/releases).


> ## Documentation Index
> Fetch the complete documentation index at: https://docs.ollama.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Errors

## Status codes

Endpoints return appropriate HTTP status codes based on the success or failure of the request in the HTTP status line (e.g. `HTTP/1.1 200 OK` or `HTTP/1.1 400 Bad Request`). Common status codes are:

* `200`: Success
* `400`: Bad Request (missing parameters, invalid JSON, etc.)
* `404`: Not Found (model doesn't exist, etc.)
* `429`: Too Many Requests (e.g. when a rate limit is exceeded)
* `500`: Internal Server Error
* `502`: Bad Gateway (e.g. when a cloud model cannot be reached)

## Error messages

Errors are returned in the `application/json` format with the following structure, with the error message in the `error` property:

```json  theme={"system"}
{
  "error": "the model failed to generate a response"
}
```

## Errors that occur while streaming

If an error occurs mid-stream, the error will be returned as an object in the `application/x-ndjson` format with an `error` property. Since the response has already started, the status code of the response will not be changed.

```json  theme={"system"}
{"model":"gemma3","created_at":"2025-10-26T17:21:21.196249Z","response":" Yes","done":false}
{"model":"gemma3","created_at":"2025-10-26T17:21:21.207235Z","response":".","done":false}
{"model":"gemma3","created_at":"2025-10-26T17:21:21.219166Z","response":"I","done":false}
{"model":"gemma3","created_at":"2025-10-26T17:21:21.231094Z","response":"can","done":false}
{"error":"an error was encountered while running the model"}
```
> ## Documentation Index
> Fetch the complete documentation index at: https://docs.ollama.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Generate a response

> Generates a response for the provided prompt



## OpenAPI

````yaml /openapi.yaml post /api/generate
openapi: 3.1.0
info:
  title: Ollama API
  version: 0.1.0
  license:
    name: MIT
    url: https://opensource.org/licenses/MIT
  description: |
    OpenAPI specification for the Ollama HTTP API
servers:
  - url: http://localhost:11434
    description: Ollama
security: []
paths:
  /api/generate:
    post:
      summary: Generate a response
      description: Generates a response for the provided prompt
      operationId: generate
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/GenerateRequest'
            example:
              model: gemma3
              prompt: Why is the sky blue?
      responses:
        '200':
          description: Generation responses
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/GenerateResponse'
              example:
                model: gemma3
                created_at: '2025-10-17T23:14:07.414671Z'
                response: Hello! How can I help you today?
                done: true
                done_reason: stop
                total_duration: 174560334
                load_duration: 101397084
                prompt_eval_count: 11
                prompt_eval_duration: 13074791
                eval_count: 18
                eval_duration: 52479709
            application/x-ndjson:
              schema:
                $ref: '#/components/schemas/GenerateStreamEvent'
      x-codeSamples:
        - lang: bash
          label: Default
          source: |
            curl http://localhost:11434/api/generate -d '{
              "model": "gemma3",
              "prompt": "Why is the sky blue?"
            }'
        - lang: bash
          label: Non-streaming
          source: |
            curl http://localhost:11434/api/generate -d '{
              "model": "gemma3",
              "prompt": "Why is the sky blue?",
              "stream": false
            }'
        - lang: bash
          label: With options
          source: |
            curl http://localhost:11434/api/generate -d '{
              "model": "gemma3",
              "prompt": "Why is the sky blue?",
              "options": {
                "temperature": 0.8,
                "top_p": 0.9,
                "seed": 42
              }
            }'
        - lang: bash
          label: Structured outputs
          source: |
            curl http://localhost:11434/api/generate -d '{
              "model": "gemma3",
              "prompt": "What are the populations of the United States and Canada?",
              "stream": false,
              "format": {
                "type": "object",
                "properties": {
                  "countries": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "properties": {
                        "country": {"type": "string"},
                        "population": {"type": "integer"}
                      },
                      "required": ["country", "population"]
                    }
                  }
                },
                "required": ["countries"]
              }
            }'
        - lang: bash
          label: With images
          source: |
            curl http://localhost:11434/api/generate -d '{
              "model": "gemma3",
              "prompt": "What is in this picture?",
              "images": [""]
            }'
        - lang: bash
          label: Load model
          source: |
            curl http://localhost:11434/api/generate -d '{
              "model": "gemma3"
            }'
        - lang: bash
          label: Unload model
          source: |
            curl http://localhost:11434/api/generate -d '{
              "model": "gemma3",
              "keep_alive": 0
            }'
components:
  schemas:
    GenerateRequest:
      type: object
      required:
        - model
      properties:
        model:
          type: string
          description: Model name
        prompt:
          type: string
          description: Text for the model to generate a response from
        suffix:
          type: string
          description: >-
            Used for fill-in-the-middle models, text that appears after the user
            prompt and before the model response
        images:
          type: array
          items:
            type: string
            description: Base64-encoded images for models that support image input
        format:
          description: >-
            Structured output format for the model to generate a response from.
            Supports either the string `"json"` or a JSON schema object.
          oneOf:
            - type: string
            - type: object
        system:
          description: System prompt for the model to generate a response from
          type: string
        stream:
          description: When true, returns a stream of partial responses
          type: boolean
          default: true
        think:
          oneOf:
            - type: boolean
            - type: string
              enum:
                - high
                - medium
                - low
          description: >-
            When true, returns separate thinking output in addition to content.
            Can be a boolean (true/false) or a string ("high", "medium", "low")
            for supported models.
        raw:
          type: boolean
          description: >-
            When true, returns the raw response from the model without any
            prompt templating
        keep_alive:
          oneOf:
            - type: string
            - type: number
          description: >-
            Model keep-alive duration (for example `5m` or `0` to unload
            immediately)
        options:
          $ref: '#/components/schemas/ModelOptions'
        logprobs:
          type: boolean
          description: Whether to return log probabilities of the output tokens
        top_logprobs:
          type: integer
          description: >-
            Number of most likely tokens to return at each token position when
            logprobs are enabled
    GenerateResponse:
      type: object
      properties:
        model:
          type: string
          description: Model name
        created_at:
          type: string
          description: ISO 8601 timestamp of response creation
        response:
          type: string
          description: The model's generated text response
        thinking:
          type: string
          description: The model's generated thinking output
        done:
          type: boolean
          description: Indicates whether generation has finished
        done_reason:
          type: string
          description: Reason the generation stopped
        total_duration:
          type: integer
          description: Time spent generating the response in nanoseconds
        load_duration:
          type: integer
          description: Time spent loading the model in nanoseconds
        prompt_eval_count:
          type: integer
          description: Number of input tokens in the prompt
        prompt_eval_duration:
          type: integer
          description: Time spent evaluating the prompt in nanoseconds
        eval_count:
          type: integer
          description: Number of output tokens generated in the response
        eval_duration:
          type: integer
          description: Time spent generating tokens in nanoseconds
        logprobs:
          type: array
          items:
            $ref: '#/components/schemas/Logprob'
          description: >-
            Log probability information for the generated tokens when logprobs
            are enabled
    GenerateStreamEvent:
      type: object
      properties:
        model:
          type: string
          description: Model name
        created_at:
          type: string
          description: ISO 8601 timestamp of response creation
        response:
          type: string
          description: The model's generated text response for this chunk
        thinking:
          type: string
          description: The model's generated thinking output for this chunk
        done:
          type: boolean
          description: Indicates whether the stream has finished
        done_reason:
          type: string
          description: Reason streaming finished
        total_duration:
          type: integer
          description: Time spent generating the response in nanoseconds
        load_duration:
          type: integer
          description: Time spent loading the model in nanoseconds
        prompt_eval_count:
          type: integer
          description: Number of input tokens in the prompt
        prompt_eval_duration:
          type: integer
          description: Time spent evaluating the prompt in nanoseconds
        eval_count:
          type: integer
          description: Number of output tokens generated in the response
        eval_duration:
          type: integer
          description: Time spent generating tokens in nanoseconds
    ModelOptions:
      type: object
      description: Runtime options that control text generation
      properties:
        seed:
          type: integer
          description: Random seed used for reproducible outputs
        temperature:
          type: number
          format: float
          description: Controls randomness in generation (higher = more random)
        top_k:
          type: integer
          description: Limits next token selection to the K most likely
        top_p:
          type: number
          format: float
          description: Cumulative probability threshold for nucleus sampling
        min_p:
          type: number
          format: float
          description: Minimum probability threshold for token selection
        stop:
          oneOf:
            - type: string
            - type: array
              items:
                type: string
          description: Stop sequences that will halt generation
        num_ctx:
          type: integer
          description: Context length size (number of tokens)
        num_predict:
          type: integer
          description: Maximum number of tokens to generate
      additionalProperties: true
    Logprob:
      type: object
      description: Log probability information for a generated token
      properties:
        token:
          type: string
          description: The text representation of the token
        logprob:
          type: number
          description: The log probability of this token
        bytes:
          type: array
          items:
            type: integer
          description: The raw byte representation of the token
        top_logprobs:
          type: array
          items:
            $ref: '#/components/schemas/TokenLogprob'
          description: Most likely tokens and their log probabilities at this position
    TokenLogprob:
      type: object
      description: Log probability information for a single token alternative
      properties:
        token:
          type: string
          description: The text representation of the token
        logprob:
          type: number
          description: The log probability of this token
        bytes:
          type: array
          items:
            type: integer
          description: The raw byte representation of the token

````

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.ollama.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Generate a chat message

> Generate the next chat message in a conversation between a user and an assistant.



## OpenAPI

````yaml /openapi.yaml post /api/chat
openapi: 3.1.0
info:
  title: Ollama API
  version: 0.1.0
  license:
    name: MIT
    url: https://opensource.org/licenses/MIT
  description: |
    OpenAPI specification for the Ollama HTTP API
servers:
  - url: http://localhost:11434
    description: Ollama
security: []
paths:
  /api/chat:
    post:
      summary: Generate a chat message
      description: >-
        Generate the next chat message in a conversation between a user and an
        assistant.
      operationId: chat
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/ChatRequest'
      responses:
        '200':
          description: Chat response
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ChatResponse'
              example:
                model: gemma3
                created_at: '2025-10-17T23:14:07.414671Z'
                message:
                  role: assistant
                  content: Hello! How can I help you today?
                done: true
                done_reason: stop
                total_duration: 174560334
                load_duration: 101397084
                prompt_eval_count: 11
                prompt_eval_duration: 13074791
                eval_count: 18
                eval_duration: 52479709
            application/x-ndjson:
              schema:
                $ref: '#/components/schemas/ChatStreamEvent'
      x-codeSamples:
        - lang: bash
          label: Default
          source: |
            curl http://localhost:11434/api/chat -d '{
              "model": "gemma3",
              "messages": [
                {
                  "role": "user",
                  "content": "why is the sky blue?"
                }
              ]
            }'
        - lang: bash
          label: Non-streaming
          source: |
            curl http://localhost:11434/api/chat -d '{
              "model": "gemma3",
              "messages": [
                {
                  "role": "user",
                  "content": "why is the sky blue?"
                }
              ],
              "stream": false
            }'
        - lang: bash
          label: Structured outputs
          source: >
            curl -X POST http://localhost:11434/api/chat -H "Content-Type:
            application/json" -d '{
              "model": "gemma3",
              "messages": [
                {
                  "role": "user",
                  "content": "What are the populations of the United States and Canada?"
                }
              ],
              "stream": false,
              "format": {
                "type": "object",
                "properties": {
                  "countries": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "properties": {
                        "country": {"type": "string"},
                        "population": {"type": "integer"}
                      },
                      "required": ["country", "population"]
                    }
                  }
                },
                "required": ["countries"]
              }
            }'
        - lang: bash
          label: Tool calling
          source: |
            curl http://localhost:11434/api/chat -d '{
              "model": "qwen3",
              "messages": [
                {
                  "role": "user",
                  "content": "What is the weather today in Paris?"
                }
              ],
              "stream": false,
              "tools": [
                {
                  "type": "function",
                  "function": {
                    "name": "get_current_weather",
                    "description": "Get the current weather for a location",
                    "parameters": {
                      "type": "object",
                      "properties": {
                        "location": {
                          "type": "string",
                          "description": "The location to get the weather for, e.g. San Francisco, CA"
                        },
                        "format": {
                          "type": "string",
                          "description": "The format to return the weather in, e.g. 'celsius' or 'fahrenheit'",
                          "enum": ["celsius", "fahrenheit"]
                        }
                      },
                      "required": ["location", "format"]
                    }
                  }
                }
              ]
            }'
        - lang: bash
          label: Thinking
          source: |
            curl http://localhost:11434/api/chat -d '{
              "model": "gpt-oss",
              "messages": [
                {
                  "role": "user",
                  "content": "What is 1+1?"
                }
              ],
              "think": "low"
            }'
        - lang: bash
          label: Images
          source: |
            curl http://localhost:11434/api/chat -d '{
              "model": "gemma3",
              "messages": [
                {
                  "role": "user",
                  "content": "What is in this image?",
                  "images": [
                    ""
                  ]
                }
              ]
            }'
components:
  schemas:
    ChatRequest:
      type: object
      required:
        - model
        - messages
      properties:
        model:
          type: string
          description: Model name
        messages:
          type: array
          description: >-
            Chat history as an array of message objects (each with a role and
            content)
          items:
            $ref: '#/components/schemas/ChatMessage'
        tools:
          type: array
          description: Optional list of function tools the model may call during the chat
          items:
            $ref: '#/components/schemas/ToolDefinition'
        format:
          oneOf:
            - type: string
              enum:
                - json
            - type: object
          description: Format to return a response in. Can be `json` or a JSON schema
        options:
          $ref: '#/components/schemas/ModelOptions'
        stream:
          type: boolean
          default: true
        think:
          oneOf:
            - type: boolean
            - type: string
              enum:
                - high
                - medium
                - low
          description: >-
            When true, returns separate thinking output in addition to content.
            Can be a boolean (true/false) or a string ("high", "medium", "low")
            for supported models.
        keep_alive:
          oneOf:
            - type: string
            - type: number
          description: >-
            Model keep-alive duration (for example `5m` or `0` to unload
            immediately)
        logprobs:
          type: boolean
          description: Whether to return log probabilities of the output tokens
        top_logprobs:
          type: integer
          description: >-
            Number of most likely tokens to return at each token position when
            logprobs are enabled
    ChatResponse:
      type: object
      properties:
        model:
          type: string
          description: Model name used to generate this message
        created_at:
          type: string
          format: date-time
          description: Timestamp of response creation (ISO 8601)
        message:
          type: object
          properties:
            role:
              type: string
              enum:
                - assistant
              description: Always `assistant` for model responses
            content:
              type: string
              description: Assistant message text
            thinking:
              type: string
              description: Optional deliberate thinking trace when `think` is enabled
            tool_calls:
              type: array
              items:
                $ref: '#/components/schemas/ToolCall'
              description: Tool calls requested by the assistant
            images:
              type: array
              items:
                type: string
              description: Optional base64-encoded images in the response
        done:
          type: boolean
          description: Indicates whether the chat response has finished
        done_reason:
          type: string
          description: Reason the response finished
        total_duration:
          type: integer
          description: Total time spent generating in nanoseconds
        load_duration:
          type: integer
          description: Time spent loading the model in nanoseconds
        prompt_eval_count:
          type: integer
          description: Number of tokens in the prompt
        prompt_eval_duration:
          type: integer
          description: Time spent evaluating the prompt in nanoseconds
        eval_count:
          type: integer
          description: Number of tokens generated in the response
        eval_duration:
          type: integer
          description: Time spent generating tokens in nanoseconds
        logprobs:
          type: array
          items:
            $ref: '#/components/schemas/Logprob'
          description: >-
            Log probability information for the generated tokens when logprobs
            are enabled
    ChatStreamEvent:
      type: object
      properties:
        model:
          type: string
          description: Model name used for this stream event
        created_at:
          type: string
          format: date-time
          description: When this chunk was created (ISO 8601)
        message:
          type: object
          properties:
            role:
              type: string
              description: Role of the message for this chunk
            content:
              type: string
              description: Partial assistant message text
            thinking:
              type: string
              description: Partial thinking text when `think` is enabled
            tool_calls:
              type: array
              items:
                $ref: '#/components/schemas/ToolCall'
              description: Partial tool calls, if any
            images:
              type: array
              items:
                type: string
              description: Partial base64-encoded images, when present
        done:
          type: boolean
          description: True for the final event in the stream
    ChatMessage:
      type: object
      required:
        - role
        - content
      properties:
        role:
          type: string
          enum:
            - system
            - user
            - assistant
            - tool
          description: Author of the message.
        content:
          type: string
          description: Message text content
        images:
          type: array
          items:
            type: string
            description: Base64-encoded image content
          description: Optional list of inline images for multimodal models
        tool_calls:
          type: array
          items:
            $ref: '#/components/schemas/ToolCall'
          description: Tool call requests produced by the model
    ToolDefinition:
      type: object
      required:
        - type
        - function
      properties:
        type:
          type: string
          enum:
            - function
          description: Type of tool (always `function`)
        function:
          type: object
          required:
            - name
            - parameters
          properties:
            name:
              type: string
              description: Function name exposed to the model
            description:
              type: string
              description: Human-readable description of the function
            parameters:
              type: object
              description: JSON Schema for the function parameters
    ModelOptions:
      type: object
      description: Runtime options that control text generation
      properties:
        seed:
          type: integer
          description: Random seed used for reproducible outputs
        temperature:
          type: number
          format: float
          description: Controls randomness in generation (higher = more random)
        top_k:
          type: integer
          description: Limits next token selection to the K most likely
        top_p:
          type: number
          format: float
          description: Cumulative probability threshold for nucleus sampling
        min_p:
          type: number
          format: float
          description: Minimum probability threshold for token selection
        stop:
          oneOf:
            - type: string
            - type: array
              items:
                type: string
          description: Stop sequences that will halt generation
        num_ctx:
          type: integer
          description: Context length size (number of tokens)
        num_predict:
          type: integer
          description: Maximum number of tokens to generate
      additionalProperties: true
    ToolCall:
      type: object
      properties:
        function:
          type: object
          required:
            - name
          properties:
            name:
              type: string
              description: Name of the function to call
            description:
              type: string
              description: What the function does
            arguments:
              type: object
              description: JSON object of arguments to pass to the function
    Logprob:
      type: object
      description: Log probability information for a generated token
      properties:
        token:
          type: string
          description: The text representation of the token
        logprob:
          type: number
          description: The log probability of this token
        bytes:
          type: array
          items:
            type: integer
          description: The raw byte representation of the token
        top_logprobs:
          type: array
          items:
            $ref: '#/components/schemas/TokenLogprob'
          description: Most likely tokens and their log probabilities at this position
    TokenLogprob:
      type: object
      description: Log probability information for a single token alternative
      properties:
        token:
          type: string
          description: The text representation of the token
        logprob:
          type: number
          description: The log probability of this token
        bytes:
          type: array
          items:
            type: integer
          description: The raw byte representation of the token

````