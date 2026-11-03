# Visual Studio Code

<div align="center">
  <img src="https://github.com/microsoft/vscode/blob/main/resources/linux/code.png?raw=true" alt="VS Code">
</div>

[![Feature Requests](https://img.shields.io/github/issues/microsoft/vscode/feature-request.svg)](https://github.com/microsoft/vscode/issues?q=is%3Aopen+is%3Aissue+label%3Afeature-request+sort%3Areactions-%2B1-desc)
[![Bugs](https://img.shields.io/github/issues/microsoft/vscode/bug.svg)](https://github.com/microsoft/vscode/issues?utf8=✓&q=is%3Aissue+is%3Aopen+label%3Abug)
[![Gitter](https://img.shields.io/badge/chat-on%20gitter-orange.svg)](https://gitter.im/Microsoft/vscode)

## The Repository

This repository ("`Code - OSS`") is where we (Microsoft) develop the [Visual Studio Code](https://code.visualstudio.com) product together with the community.

## Visual Studio Code

<a href="https://code.visualstudio.com">
<img alt="VS Code in action" src="https://github.com/microsoft/vscode/blob/main/docs/images/hero.png?raw=true">
</a>

[Visual Studio Code](https://code.visualstudio.com) is a lightweight but powerful source code editor which runs on your desktop and is available for Windows, macOS and Linux. It comes with built-in support for JavaScript, TypeScript and Node.js and has a rich ecosystem of extensions for other languages and runtimes.

## Contributing

There are many ways in which you can participate in this project, for example:

- [Submit bugs and feature requests](https://github.com/microsoft/vscode/issues)
- Review [source code changes](https://github.com/microsoft/vscode/pulls)
- Review and comment on [documentation](https://github.com/microsoft/vscode-docs)

### Building

#### Prerequisites

- [Node.js](https://nodejs.org/en/)
- [Python](https://www.python.org/)
- Git
- C++ build tools (Windows: Visual Studio, macOS: Xcode, Linux: GCC)

#### Steps

```bash
git clone https://github.com/microsoft/vscode.git
cd vscode
./scripts/code.sh  # Linux/macOS
.\scripts\code.bat  # Windows
```

## Related Projects

Many of the core components and extensions to VS Code live in their own repositories on GitHub.

## License

Copyright (c) Microsoft Corporation. All rights reserved.

Licensed under the [MIT](https://github.com/microsoft/vscode/blob/main/LICENSE.txt) License.
