# HTML and Inline HTML Test

## Raw HTML Blocks

<div class="container">
  <h1>HTML Heading</h1>
  <p>This is a paragraph inside HTML.</p>
</div>

<table border="1">
  <tr>
    <th>Column 1</th>
    <th>Column 2</th>
  </tr>
  <tr>
    <td>Data 1</td>
    <td>Data 2</td>
  </tr>
</table>

<details>
  <summary>Click to expand</summary>
  <p>This content is hidden by default.</p>
  <ul>
    <li>Item 1</li>
    <li>Item 2</li>
  </ul>
</details>

<progress value="70" max="100">70%</progress>

<meter value="0.6">60%</meter>

## Inline HTML

This is a paragraph with <strong>inline HTML</strong> mixed with Markdown.

You can use <mark>highlighted text</mark> inline.

Here's a <kbd>Ctrl</kbd> + <kbd>C</kbd> keyboard shortcut.

This is <sub>subscript</sub> and this is <sup>superscript</sup>.

Use <code>inline code</code> or <code>preformatted</code> text.

## HTML with Attributes

<div style="background-color: #f0f0f0; padding: 20px; border-radius: 5px;">
  <p>This div has inline styles.</p>
</div>

<span title="Tooltip text">Hover over me</span>

<a href="https://example.com" target="_blank" rel="noopener noreferrer">External link</a>

## HTML Entities

Copyright: &copy;
Registered: &reg;
Trademark: &trade;
Euro: &euro;
Pound: &pound;
Yen: &yen;
Less than: &lt;
Greater than: &gt;
Ampersand: &amp;
Non-breaking space:&nbsp;
Em dash: &mdash;
En dash: &ndash;

## HTML Comments

<!-- This is an HTML comment -->

This text is visible.

<!-- 
  This is a 
  multi-line comment
-->

More visible text.

## Mixed HTML and Markdown

<div markdown="1">
This is **Markdown** inside HTML with markdown="1" attribute.

- List item 1
- List item 2
</div>

<details markdown="1">
<summary>Click me</summary>

This is **Markdown** content inside details.

```python
print("Code block inside details")
```

</details>

## HTML Forms (Display Only)

<form>
  <label for="name">Name:</label>
  <input type="text" id="name" name="name"><br><br>
  
  <label for="email">Email:</label>
  <input type="email" id="email" name="email"><br><br>
  
  <label for="message">Message:</label><br>
  <textarea id="message" name="message" rows="4" cols="50"></textarea><br><br>
  
  <input type="submit" value="Submit">
</form>

## HTML5 Semantic Elements

<article>
  <header>
    <h2>Article Title</h2>
    <p>Posted on <time datetime="2024-01-15">January 15, 2024</time></p>
  </header>
  
  <section>
    <p>This is the article content.</p>
  </section>
  
  <footer>
    <p>Written by <address>John Doe</address></p>
  </footer>
</article>

<nav>
  <ul>
    <li><a href="#home">Home</a></li>
    <li><a href="#about">About</a></li>
    <li><a href="#contact">Contact</a></li>
  </ul>
</nav>

<aside>
  <h3>Related Links</h3>
  <ul>
    <li><a href="#">Link 1</a></li>
    <li><a href="#">Link 2</a></li>
  </ul>
</aside>

## HTML with Markdown Inside

<div>

This is **Markdown** inside a div (with blank lines).

- List item 1
- List item 2

</div>

## Self-Closing Tags

<br/>
<hr/>
<img src="https://example.com/image.png" alt="Example" />
<input type="text" />

## HTML in Lists

- Item with <strong>bold HTML</strong>
- Item with <em>italic HTML</em>
- Item with <code>code HTML</code>
- Item with <a href="#">link HTML</a>

## HTML Tables with Markdown

<table>
  <thead>
    <tr>
      <th>Feature</th>
      <th>Status</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td>Feature 1</td>
      <td>✅ Complete</td>
    </tr>
    <tr>
      <td>Feature 2</td>
      <td>🚧 In Progress</td>
    </tr>
  </tbody>
</table>

## Inline HTML Edge Cases

Text before<br>text after line break.

First paragraph.</p><p>Second paragraph (forced with HTML).

<span style="color: red;">Red text</span> and <span style="color: blue;">blue text</span>.

<div style="display: inline-block; width: 100px; height: 100px; background: #ff0000;"></div>
