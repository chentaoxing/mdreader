# Images and Links Test

## Inline Images

![Alt text](https://example.com/image.png)

![Logo](https://example.com/logo.svg "Optional title")

![Local image](./images/local.png)

![Relative path](../assets/image.jpg)

## Image with HTML

<img src="https://example.com/image.png" alt="HTML image" width="500" />

<img src="image.png" alt="Responsive" style="max-width: 100%; height: auto;" />

## Image Links

[![Click me](https://example.com/button.png)](https://example.com)

[![Logo](logo.png "Logo Title")](https://example.com "Example Site")

## Inline Links

[Link text](https://example.com)

[Link with title](https://example.com "Link title")

[Relative link](./page.md)

[Anchor link](#section-name)

## Reference Links

This is a [reference link][1].

This is a [link with title][2].

This is an [implicit reference][].

[1]: https://example.com
[2]: https://example.com/page "Page title"
[implicit reference]: https://example.com/implicit

## Autolinks

<https://example.com>

<user@example.com>

<ftp://ftp.example.com>

## Automatic Link Detection

Visit https://example.com for more info.

Contact us at support@example.com.

## Link Edge Cases

[Link with parentheses](https://en.wikipedia.org/wiki/Markdown_(software))

[Link with special chars](https://example.com/path?query=value&other=123)

[Link with fragment](https://example.com/page#section)

[Empty link]()

[Link with spaces](https://example.com/path%20with%20spaces)

## Mixed Link Types

Here's a paragraph with [inline link](https://example.com), [reference link][ref], and <autolink@example.com>.

[ref]: https://example.com/reference

## Images with Different Formats

![PNG](image.png)
![JPEG](photo.jpg)
![GIF](animation.gif)
![SVG](vector.svg)
![WebP](modern.webp)

## Image Sizes (HTML)

<img src="small.png" width="100" height="100" alt="Small" />
<img src="medium.png" width="300" alt="Medium (auto height)" />
<img src="large.png" style="width: 50%;" alt="Percentage width" />

## Linked Images

[![Badge](https://img.shields.io/badge/version-1.0-blue)](https://example.com)

[![Build Status](https://example.com/badge.svg)](https://example.com/ci)

## Complex Image Scenarios

![Alt text with **markdown**](image.png)

![Alt with [link]](image.png)

![Very long alt text that describes the image in great detail and might wrap across multiple lines when rendered](image.png)

## Image References

![Reference image][img1]

![Another reference][img2]

[img1]: https://example.com/image1.png "First image"
[img2]: https://example.com/image2.png "Second image"

## Broken Images

![Broken](https://example.com/nonexistent.png)

![Invalid URL](not-a-valid-url)

## Data URI Images

![Data URI](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==)

## Email Links

[Contact us](mailto:support@example.com)

[Email with subject](mailto:support@example.com?subject=Help)

[Email with body](mailto:support@example.com?subject=Help&body=I%20need%20assistance)

## Phone Links

[Call us](tel:+1-555-123-4567)

[Text us](sms:+1-555-123-4567?body=Hello)

## Download Links

[Download PDF](document.pdf "Download")

[Download ZIP](archive.zip)

## Anchor Links

[Back to top](#top)

[Go to section](#section-1)

[Jump to footnote](#fn1)

## Link with Code

[`code`](https://example.com)

[**bold link**](https://example.com)

[*italic link*](https://example.com)

## Multiple Links in Paragraph

Visit [Google](https://google.com), [GitHub](https://github.com), or [Stack Overflow](https://stackoverflow.com) for more information. You can also check the [documentation](https://docs.example.com).

## Image Gallery (HTML)

<div style="display: flex; gap: 10px;">
  <img src="image1.png" alt="Image 1" width="200" />
  <img src="image2.png" alt="Image 2" width="200" />
  <img src="image3.png" alt="Image 3" width="200" />
</div>
