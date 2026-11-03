# Complex Tables Test

This document tests various table formats and edge cases.

## Basic Table

| Name | Age | City |
|------|-----|------|
| Alice | 30 | New York |
| Bob | 25 | San Francisco |
| Charlie | 35 | London |

## Aligned Columns

| Left | Center | Right |
|:-----|:------:|------:|
| left1 | center1 | right1 |
| left2 | center2 | right2 |
| left3 | center3 | right3 |

## Table with Markdown in Cells

| Feature | Description | Example |
|---------|-------------|---------|
| **Bold** | Text with **bold** formatting | `**text**` |
| *Italic* | Text with *italic* formatting | `*text*` |
| `Code` | Inline `code` blocks | `` `code` `` |
| [Link](https://example.com) | Hyperlinks | `[text](url)` |

## Table with Long Content

| ID | Description | Notes |
|----|-------------|-------|
| 1 | This is a very long description that spans multiple words and should be handled correctly in table rendering | Some notes here |
| 2 | Another long description with special characters: @#$%^&*() | More notes |
| 3 | Short | N/A |

## Table with Empty Cells

| Column A | Column B | Column C |
|----------|----------|----------|
| Value 1 | | Value 3 |
| | Value 2 | |
| Value 4 | Value 5 | Value 6 |

## Complex Nested Content

| Category | Items | Status |
|----------|-------|--------|
| Fruits | - Apple<br>- Banana<br>- Orange | ✅ Available |
| Vegetables | - Carrot<br>- Broccoli<br>- Spinach | ⚠️ Limited |
| Dairy | - Milk<br>- Cheese<br>- Yogurt | ❌ Out of stock |

## Table with Code Blocks

| Language | Example | Description |
|----------|---------|-------------|
| Python | `print("Hello")` | Print function |
| JavaScript | `console.log("Hi")` | Console logging |
| Rust | `println!("Hey")` | Print with newline |

## Wide Table

| Column 1 | Column 2 | Column 3 | Column 4 | Column 5 | Column 6 | Column 7 | Column 8 |
|----------|----------|----------|----------|----------|----------|----------|----------|
| Data 1-1 | Data 1-2 | Data 1-3 | Data 1-4 | Data 1-5 | Data 1-6 | Data 1-7 | Data 1-8 |
| Data 2-1 | Data 2-2 | Data 2-3 | Data 2-4 | Data 2-5 | Data 2-6 | Data 2-7 | Data 2-8 |
| Data 3-1 | Data 3-2 | Data 3-3 | Data 3-4 | Data 3-5 | Data 3-6 | Data 3-7 | Data 3-8 |

## Table with Special Characters

| Symbol | Name | Unicode |
|--------|------|---------|
| © | Copyright | U+00A9 |
| ® | Registered | U+00AE |
| ™ | Trademark | U+2122 |
| € | Euro | U+20AC |
| £ | Pound | U+00A3 |
| ¥ | Yen | U+00A5 |

## Table with Numbers

| Integer | Float | Scientific | Percentage |
|---------|-------|------------|------------|
| 123 | 3.14159 | 1.23e10 | 99.9% |
| -456 | -2.71828 | 4.56e-5 | 50.0% |
| 789 | 0.001 | 7.89e+2 | 0.1% |

## Table with Mixed Alignment

| Left | Center | Right | Left |
|:-----|:------:|------:|:-----|
| L1 | C1 | R1 | L1 |
| L2 | C2 | R2 | L2 |
| L3 | C3 | R3 | L3 |

## Table with Line Breaks

| Name | Address | Contact |
|------|---------|---------|
| John Doe | 123 Main St<br>Apt 4B<br>New York, NY 10001 | john@example.com<br>+1-555-0123 |
| Jane Smith | 456 Oak Ave<br>San Francisco, CA 94102 | jane@example.com |

## Comparison Table

| Feature | Basic | Pro | Enterprise |
|---------|:-----:|:---:|:----------:|
| Users | 1-5 | 6-50 | Unlimited |
| Storage | 10GB | 100GB | 1TB |
| Support | Email | Priority | 24/7 |
| API Access | ❌ | ✅ | ✅ |
| Custom Domain | ❌ | ❌ | ✅ |
| Price | $9/mo | $29/mo | $99/mo |
