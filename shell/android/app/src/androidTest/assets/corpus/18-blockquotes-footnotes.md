# Blockquotes and Footnotes

## Basic Blockquotes

> This is a simple blockquote.

> This is a blockquote with multiple lines.
> It continues on the second line.
> And even a third line.

## Nested Blockquotes

> This is the first level of quoting.
>
> > This is a nested blockquote.
>
> Back to the first level.

> Level 1
> > Level 2
> > > Level 3
> > > > Level 4
> > > Back to level 3
> > Back to level 2
> Back to level 1

## Blockquotes with Markdown

> # This is a heading in a blockquote
>
> This is a paragraph with **bold** and *italic* text.
>
> - List item 1
> - List item 2
>   - Nested item
>
> ```python
> def example():
>     return "Code block in blockquote"
> ```
>
> [This is a link](https://example.com)

## Blockquotes with HTML

> <div>
>   <p>This is HTML inside a blockquote.</p>
>   <ul>
>     <li>Item 1</li>
>     <li>Item 2</li>
>   </ul>
> </div>

## Famous Quotes

> "The only way to do great work is to love what you do."
> 
> — Steve Jobs

> "Innovation distinguishes between a leader and a follower."
> 
> — Steve Jobs

> "Stay hungry, stay foolish."
> 
> — Steve Jobs

## Blockquote with Attribution

> This is a quote from a famous person. It can be quite long and span multiple lines to demonstrate how blockquotes handle longer content.
>
> <footer>— Famous Person, <cite>Source Book</cite></footer>

## Footnotes

This is a paragraph with a footnote[^1]. This is another sentence with a different footnote[^2].

Here's a more complex example with multiple footnotes in one paragraph[^3]. And here's another one[^4].

[^1]: This is the first footnote. It can contain **markdown** formatting.

[^2]: This is the second footnote.
    
    It can span multiple paragraphs.

[^3]: Footnotes can contain code blocks:
    
    ```python
    def example():
        return "Hello from footnote"
    ```

[^4]: This footnote has a list:
    
    - Item 1
    - Item 2
    - Item 3

## Inline Footnotes

This is text with an inline note^[This is an inline footnote].

## Footnote References

You can reference the same footnote multiple times[^5].

Here's another reference to the same footnote[^5].

And one more time[^5].

[^5]: This footnote is referenced three times.

## Complex Footnotes

Here's a paragraph with a complex footnote[^6].

[^6]: This footnote contains:
    
    **Bold text** and *italic text*.
    
    A code example:
    ```javascript
    console.log("Hello");
    ```
    
    And a list:
    1. First item
    2. Second item
    3. Third item
    
    > Even a blockquote!

## Blockquote Edge Cases

>This is a blockquote without space after >

> This blockquote has a blank line
>
> in the middle

>Blockquote
>with
>multiple
>short
>lines

> A very long line that should wrap properly when rendered: Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.

## Combined: Blockquotes and Footnotes

> This blockquote contains a footnote[^7].
>
> > And this nested blockquote has another one[^8].

[^7]: First footnote in blockquote.

[^8]: Second footnote in nested blockquote.

## Blockquote with Task List

> - [x] Completed task in blockquote
> - [ ] Incomplete task in blockquote
> - [x] Another completed task
