# React

[![GitHub stars](https://img.shields.io/github/stars/facebook/react.svg?style=social&label=Star)](https://github.com/facebook/react/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/facebook/react.svg?style=social&label=Fork)](https://github.com/facebook/react/network)

The library for web and native user interfaces.

## Installation

```bash
npm install react react-dom
```

## Documentation

Learn how to use React on [the official website](https://react.dev).

## Examples

### Functional Component

```jsx
function Welcome({ name }) {
  return <h1>Hello, {name}!</h1>;
}

export default function App() {
  return (
    <div>
      <Welcome name="React" />
      <p>Welcome to the React documentation.</p>
    </div>
  );
}
```

### Hooks

```jsx
import { useState, useEffect } from 'react';

function Counter() {
  const [count, setCount] = useState(0);

  useEffect(() => {
    document.title = `You clicked ${count} times`;
  }, [count]);

  return (
    <div>
      <p>You clicked {count} times</p>
      <button onClick={() => setCount(count + 1)}>
        Click me
      </button>
    </div>
  );
}
```

### Class Component (Legacy)

```jsx
class Welcome extends React.Component {
  render() {
    return <h1>Hello, {this.props.name}!</h1>;
  }
}
```

## Contributing

The main purpose of this repository is to continue evolving React core.

### Development

```bash
git clone https://github.com/facebook/react.git
cd react
yarn install
yarn build
```

## License

MIT
