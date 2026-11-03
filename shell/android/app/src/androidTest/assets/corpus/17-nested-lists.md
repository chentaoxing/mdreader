# Nested Lists and Task Lists

## Nested Unordered Lists

- Item 1
  - Subitem 1.1
    - Sub-subitem 1.1.1
    - Sub-subitem 1.1.2
  - Subitem 1.2
- Item 2
  - Subitem 2.1
  - Subitem 2.2
    - Sub-subitem 2.2.1
- Item 3

## Nested Ordered Lists

1. First item
   1. Subitem 1.1
   2. Subitem 1.2
      1. Sub-subitem 1.2.1
      2. Sub-subitem 1.2.2
   3. Subitem 1.3
2. Second item
   1. Subitem 2.1
   2. Subitem 2.2
3. Third item

## Mixed Nested Lists

- Unordered item 1
  1. Ordered subitem 1.1
  2. Ordered subitem 1.2
     - Unordered sub-subitem 1.2.1
     - Unordered sub-subitem 1.2.2
  3. Ordered subitem 1.3
- Unordered item 2
  - Unordered subitem 2.1
  - Unordered subitem 2.2
    1. Ordered sub-subitem 2.2.1
    2. Ordered sub-subitem 2.2.2

## GFM Task Lists

### Simple Task List

- [x] Completed task 1
- [x] Completed task 2
- [ ] Incomplete task 3
- [ ] Incomplete task 4
- [x] Completed task 5

### Nested Task List

- [x] Project setup
  - [x] Initialize repository
  - [x] Configure CI/CD
  - [ ] Set up monitoring
- [ ] Development
  - [x] Design database schema
  - [ ] Implement API endpoints
    - [ ] User authentication
    - [ ] Data validation
    - [ ] Error handling
  - [ ] Write unit tests
- [ ] Deployment
  - [ ] Configure production environment
  - [ ] Deploy to staging
  - [ ] Run integration tests
  - [ ] Deploy to production

### Task List with Descriptions

- [x] **Research phase**
  - Gather requirements from stakeholders
  - Analyze existing systems
  - Document findings
- [ ] **Design phase**
  - Create wireframes
  - Design database schema
  - Plan API structure
- [ ] **Implementation phase**
  - Set up development environment
  - Write core functionality
  - Add error handling
- [ ] **Testing phase**
  - Unit tests
  - Integration tests
  - User acceptance testing

## Lists with Code

- Install dependencies:
  ```bash
  npm install
  ```
- Run the application:
  ```bash
  npm start
  ```
- Build for production:
  ```bash
  npm run build
  ```

## Lists with Links

- Documentation
  - [Getting Started](https://example.com/start)
  - [API Reference](https://example.com/api)
  - [Examples](https://example.com/examples)
- Resources
  - [GitHub Repository](https://github.com/example/repo)
  - [Issue Tracker](https://github.com/example/repo/issues)

## Deeply Nested Lists

- Level 1
  - Level 2
    - Level 3
      - Level 4
        - Level 5
          - Level 6
            - Level 7
              - Level 8
                - Level 9
                  - Level 10

## Lists with Blockquotes

- First item
  > This is a blockquote inside a list item
  > 
  > It can span multiple lines
- Second item
  > Another blockquote
- Third item

## Lists with Multiple Paragraphs

- First item with multiple paragraphs

  This is the second paragraph of the first item.
  
  And this is the third paragraph.

- Second item

  This is the second paragraph of the second item.

- Third item

## Lists with Inline Formatting

- **Bold item**
- *Italic item*
- `Code item`
- ~~Strikethrough item~~
- **Combined: *bold and italic***
- [Link item](https://example.com)
