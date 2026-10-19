# Node.js

Node.js is an open-source, cross-platform JavaScript runtime environment.

## Quick start

### Download and install

```bash
curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -
sudo apt-get install -y nodejs
```

### Create and run a server

```js
const http = require('http');

const hostname = '127.0.0.1';
const port = 3000;

const server = http.createServer((req, res) => {
  res.statusCode = 200;
  res.setHeader('Content-Type', 'text/plain');
  res.end('Hello World\n');
});

server.listen(port, hostname, () => {
  console.log(`Server running at http://${hostname}:${port}/`);
});
```

## Features

- **Asynchronous event-driven model**
- **Non-blocking I/O operations**
- **V8 JavaScript engine**
- **Cross-platform support**

## Resources

- [Official Website](https://nodejs.org)
- [Documentation](https://nodejs.org/docs/latest/api/)
- [GitHub Repository](https://github.com/nodejs/node)

## License

MIT
