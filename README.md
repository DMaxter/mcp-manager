# MCP Manager

This tool aims at being a generic client for Chat Completion AI models, allowing them to perform some tasks using MCP servers.

We can define multiple workspaces with different configurations, meaning we can have different endpoints with different models and same MCP servers and exposed on different ports.
All of this can be controlled through the configuration file.

## Configuration

The configuration is done through a YAML file. By default, it is named `config.yaml` and should be present in the same folder as where the program is running.

The configuration file can be changed by setting the environment variable `MCP_MANAGER_CONFIG` to the appropriate path.

An example of the configuration can be found at [config.yaml.example](./config.yaml.example) with comments.

## Usage

Once the server is running, and assuming it is listening on port `7000` (default) and we configured a path `/test`, we can make our calls with curl, like:

```sh
curl http://localhost:7000/test -H "Content-Type: application/json" -d '"List my files in /tmp"'
```

## Limitations

At the moment, only one prompt is available for each workspace, but in the future, should be possible to use OpenAI request bodies

### Supported models

At the moment, support is only provided to:

* Gemini (only [OpenAI compatible calls](https://ai.google.dev/gemini-api/docs/openai) at the moment)
* OpenAI-compatible models

It is also planned to support further models in the future (See [Claude](https://gitlab.com/DMaxter/mcp-manager/-/issues/3) and [Gemini](https://gitlab.com/DMaxter/mcp-manager/-/issues/1))

### Supported MCP servers

Currently, only local MCP servers are supported, but it is planned to support [remote MCP servers](https://gitlab.com/DMaxter/mcp-manager/-/issues/5) as well

## Contributing

Contributions are welcome, see [CONTRIBUTING.md](./CONTRIBUTING.md)

Main development is done at [GitLab](https://gitlab.com/DMaxter/mcp-manager) while a mirror is maintained at [GitHub](https://github.com/DMaxter/mcp-manager). Issues on GitHub will be ported to GitLab manually.
