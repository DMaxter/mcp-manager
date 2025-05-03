# MCP Manager

[![CI](https://gitlab.com/DMaxter/mcp-manager/badges/main/pipeline.svg)](https://gitlab.com/DMaxter/mcp-manager/-/pipelines?page=1&scope=all&ref=main)
[![Release](https://gitlab.com/DMaxter/mcp-manager/-/badges/release.svg)](https://gitlab.com/DMaxter/mcp-manager/-/releases/permalink/latest)

MCP Manager acts as a bridge between Large Language Models (LLMs) and MCP servers. It allows you to interact with remote and local APIs using natural language prompts via supported chat completion APIs.

Workspaces can be defined with distinct configurations, enabling connections to different LLM endpoints and MCP servers (potentially on various ports, addresses, or paths) using a single MCP Manager instance.

### What is an MCP Server?

An MCP server is a middleware that currently sits between LLMs and APIs allowing the LLM to check which tools it has available to perform actions that the user prompt might have and decide to act instead of providing only an answer to the user. An MCP server translates this "will" of the model to perform an action into the action itself, might it be a change or a query to some system.

If you want to learn more, head over to <https://mcp.so> and scroll to the bottom for the FAQ section.


## Features

* Integrates with various LLMs (Gemini, Azure OpenAI currently supported)
* Enables LLM interaction with MCP servers
* Flexible workspace configuration via a single YAML file
* Supports API Key authentication (OAuth 2.0 planned)
* Exposes a simple HTTP API for sending prompts, in the OpenAI format

## Installation

Just download the appropriate file for your operating system in the [Release](https://gitlab.com/DMaxter/mcp-manager/-/releases) section, on **Packages**, and it is ready to go.

## Configuration

The configuration is managed through a YAML file (default: `config.yaml` in the runtime directory). The path can be overriden using the `MCP_MANAGER_CONFIG` environment variable.

An annotated example configuration file is available at [config.example.yaml](./config.example.yaml).

### Authentication

Currently, only API Key authentication is supported and is configured within the model settings. [OAuth 2.0 support](https://gitlab.com/DMaxter/mcp-manager/-/issues/17) is planned.


### LLM Configuration

Configuration varies depending on the LLM provider:
* **Gemini**
    * Requires an [API Key](https://ai.google.dev/gemini-api/docs/api-key)
    * The API endpoint can be found in the [Gemini documentation](https://ai.google.dev/gemini-api/docs/function-calling?example=chart#rest_2)(use the base REST endpoint). The API Key **should be configured via MCP Manager** and **not included in the URL**

* **Azure OpenAI**
    * Requires a deployed model
    * Resource endpoint
    * API Version (see [available versions](https://github.com/Azure/azure-rest-api-specs/tree/main/specification/cognitiveservices/data-plane/AzureOpenAI/inference))
    * Deployed model name
    * API Key

## Usage

1. Start the server

```bash
mcp-manager
```

Optionally with a different configuration path
```bash
export MCP_MANAGER_CONFIG=/path/to/your/config.yaml
mcp-manager
```

2. Perform prompts via HTTP call (assuming default port)

Example with curl, using the workspace configured for `/azure` and using the filesystem MCP server:
```bash
curl http://localhost:7000/azure -H "Content-Type: application/json" -d '{"messages": [{"role":"user","content":"Check if the file /tmp/abc exists"}]}'
```

The output is something similar to:
```json
{
  "messages": [
    {
      "role": "user",
      "content": "Check if the file /tmp/abc exists"
    },
    {
      "role": "assistant",
      "tool_calls": [
        {
          "name": "get_file_info",
          "id": "cGAjCuzqBBnUx2J2dpjpDbZg",
          "arguments": {
            "path": "/tmp/abc"
          }
        }
      ]
    },
    {
      "type": "FunctionCallOutput",
      "call_id": "cGAjCuzqBBnUx2J2dpjpDbZg",
      "output": "Error: ENOENT: no such file or directory, stat '/tmp/abc'"
    },
    {
      "role": "assistant",
      "content": "The file /tmp/abc does not exist."
    }
  ],
  "temperature": null,
  "max_tokens": null,
  "top_p": null,
  "tools": null
}
```

We get a complete list of all the messages exchanged between the user, the model, MCP Manager and the MCP servers.

## Limitations

* **Supported LLMs**
    * Gemini
    * Azure OpenAI
    * (Planned: [Claude](https://gitlab.com/DMaxter/mcp-manager/-/issues/3) and [OpenAI](https://gitlab.com/DMaxter/mcp-manager/-/issues/2))

* **Supported MCP Server connections**
    * Local MCP servers
    * (Planned: [Remote MCP servers](https://gitlab.com/DMaxter/mcp-manager/-/issues/5))

## Contributing

Contributions are welcome, see [CONTRIBUTING.md](./CONTRIBUTING.md)

Main development occurs on [GitLab](https://gitlab.com/DMaxter/mcp-manager). Issues and merge requests should be submitted there. A mirror repository is maintained on [GitHub](https://github.com/DMaxter/mcp-manager) and issues opened on GitHub will be manually migrated to GitLab.

Bug reports and feature requests are encouraged via [GitLab Issues](https://gitlab.com/DMaxter/mcp-manager/-/issues).

## License

This project is licensed under GNU GPLv3
