# Vue.js

<p align="center"><a href="https://vuejs.org" target="_blank" rel="noopener noreferrer"><img width="100" src="https://vuejs.org/images/logo.png" alt="Vue logo"></a></p>

<p align="center">
  <a href="https://circleci.com/gh/vuejs/vue/tree/dev"><img src="https://img.shields.io/circleci/project/github/vuejs/vue/dev.svg?sanitize=true" alt="Build Status"></a>
  <a href="https://codecov.io/github/vuejs/vue?branch=dev"><img src="https://img.shields.io/codecov/c/github/vuejs/vue/dev.svg?sanitize=true" alt="Coverage Status"></a>
  <a href="https://npmcharts.com/compare/vue?minimal=true"><img src="https://img.shields.io/npm/dm/vue.svg?sanitize=true" alt="Downloads"></a>
  <a href="https://www.npmjs.com/package/vue"><img src="https://img.shields.io/npm/v/vue.svg?sanitize=true" alt="Version"></a>
  <a href="https://www.npmjs.com/package/vue"><img src="https://img.shields.io/npm/l/vue.svg?sanitize=true" alt="License"></a>
</p>

## Introduction

Vue (pronounced /vjuː/, similar to **view**) is a **progressive framework** for building user interfaces. It is designed from the ground up to be incrementally adoptable.

### Core Features

- **Declarative Rendering**: Vue extends standard HTML with a template syntax that allows us to declaratively describe HTML output based on JavaScript state.
- **Reactivity**: Vue automatically tracks JavaScript state changes and efficiently updates the DOM.
- **Component System**: Build encapsulated components that manage their own state, then compose them to make complex applications.

## Getting Started

### Installation

```bash
npm install vue
```

### Basic Example

```html
<div id="app">
  {{ message }}
</div>

<script>
  const app = Vue.createApp({
    data() {
      return {
        message: 'Hello Vue!'
      }
    }
  })
  
  app.mount('#app')
</script>
```

## Documentation

To check out **live examples** and docs, visit [vuejs.org](https://vuejs.org).

## Questions

For questions and support please use [the official forum](https://forum.vuejs.org) or [community chat](https://chat.vuejs.org).

## License

[MIT](https://opensource.org/licenses/MIT)

Copyright (c) 2013-present, Yuxi (Evan) You
