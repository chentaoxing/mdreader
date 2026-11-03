# Complex Code Blocks Test

This document contains extensive code blocks in multiple languages.

## Python

```python
#!/usr/bin/env python3
"""
A complex Python example with multiple features.
"""

import os
import sys
from typing import List, Dict, Optional
from dataclasses import dataclass

@dataclass
class Person:
    name: str
    age: int
    email: Optional[str] = None
    
    def greet(self) -> str:
        return f"Hello, my name is {self.name}"

class Company:
    def __init__(self, name: str):
        self.name = name
        self.employees: List[Person] = []
    
    def add_employee(self, person: Person) -> None:
        self.employees.append(person)
    
    def get_employee_by_name(self, name: str) -> Optional[Person]:
        for emp in self.employees:
            if emp.name == name:
                return emp
        return None

def main():
    company = Company("Tech Corp")
    company.add_employee(Person("Alice", 30, "alice@example.com"))
    company.add_employee(Person("Bob", 25))
    
    for emp in company.employees:
        print(emp.greet())

if __name__ == "__main__":
    main()
```

## JavaScript / TypeScript

```javascript
// Modern JavaScript with async/await
class DataProcessor {
  constructor(config) {
    this.config = config;
    this.data = [];
  }
  
  async fetchData(url) {
    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      const data = await response.json();
      this.data = data;
      return data;
    } catch (error) {
      console.error('Fetch failed:', error);
      throw error;
    }
  }
  
  processData() {
    return this.data
      .filter(item => item.active)
      .map(item => ({
        ...item,
        processed: true,
        timestamp: Date.now()
      }))
      .sort((a, b) => b.priority - a.priority);
  }
}

// Usage
const processor = new DataProcessor({ timeout: 5000 });
processor.fetchData('https://api.example.com/data')
  .then(() => {
    const processed = processor.processData();
    console.log(processed);
  });
```

```typescript
// TypeScript with generics and interfaces
interface ApiResponse<T> {
  data: T;
  status: number;
  message: string;
}

interface User {
  id: number;
  name: string;
  email: string;
  roles: string[];
}

class ApiClient {
  private baseUrl: string;
  
  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }
  
  async get<T>(endpoint: string): Promise<ApiResponse<T>> {
    const response = await fetch(`${this.baseUrl}${endpoint}`);
    return response.json();
  }
  
  async post<T>(endpoint: string, data: unknown): Promise<ApiResponse<T>> {
    const response = await fetch(`${this.baseUrl}${endpoint}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data)
    });
    return response.json();
  }
}

// Usage
const client = new ApiClient('https://api.example.com');
client.get<User[]>('/users').then(response => {
  console.log(response.data);
});
```

## Rust

```rust
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::error::Error;

#[derive(Debug, Clone)]
struct Config {
    name: String,
    version: String,
    settings: HashMap<String, String>,
}

impl Config {
    fn new(name: &str, version: &str) -> Self {
        Config {
            name: name.to_string(),
            version: version.to_string(),
            settings: HashMap::new(),
        }
    }
    
    fn set(&mut self, key: &str, value: &str) {
        self.settings.insert(key.to_string(), value.to_string());
    }
    
    fn get(&self, key: &str) -> Option<&String> {
        self.settings.get(key)
    }
}

fn process_data<T: AsRef<str>>(data: &[T]) -> Vec<String> {
    data.iter()
        .map(|item| item.as_ref().to_uppercase())
        .filter(|s| s.len() > 3)
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut config = Config::new("MyApp", "1.0.0");
    config.set("debug", "true");
    config.set("timeout", "30");
    
    if let Some(debug) = config.get("debug") {
        println!("Debug mode: {}", debug);
    }
    
    let data = vec!["hello", "world", "rust", "programming"];
    let processed = process_data(&data);
    println!("Processed: {:?}", processed);
    
    Ok(())
}
```

## Shell / Bash

```bash
#!/bin/bash
set -euo pipefail

# Configuration
readonly SCRIPT_NAME="$(basename "$0")"
readonly LOG_FILE="/var/log/${SCRIPT_NAME}.log"
readonly MAX_RETRIES=3

# Logging function
log() {
    local level="$1"
    shift
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] [$level] $*" | tee -a "$LOG_FILE"
}

# Error handler
error_handler() {
    local exit_code=$?
    local line_number=$1
    log "ERROR" "Script failed at line $line_number with exit code $exit_code"
    exit $exit_code
}

trap 'error_handler $LINENO' ERR

# Main function
main() {
    log "INFO" "Starting script execution"
    
    local retry_count=0
    while (( retry_count < MAX_RETRIES )); do
        if some_command; then
            log "INFO" "Command succeeded"
            break
        else
            ((retry_count++))
            log "WARN" "Command failed, retry $retry_count of $MAX_RETRIES"
            sleep 2
        fi
    done
    
    if (( retry_count == MAX_RETRIES )); then
        log "ERROR" "Command failed after $MAX_RETRIES retries"
        exit 1
    fi
    
    log "INFO" "Script completed successfully"
}

main "$@"
```

## JSON

```json
{
  "name": "example-project",
  "version": "1.0.0",
  "description": "A sample project configuration",
  "main": "index.js",
  "scripts": {
    "start": "node index.js",
    "test": "jest",
    "build": "webpack --mode production",
    "lint": "eslint src/**/*.js"
  },
  "dependencies": {
    "express": "^4.18.2",
    "lodash": "^4.17.21"
  },
  "devDependencies": {
    "jest": "^29.5.0",
    "eslint": "^8.40.0",
    "webpack": "^5.85.0"
  },
  "keywords": ["example", "node", "javascript"],
  "author": "John Doe <john@example.com>",
  "license": "MIT"
}
```

## YAML

```yaml
# Kubernetes deployment configuration
apiVersion: apps/v1
kind: Deployment
metadata:
  name: myapp-deployment
  labels:
    app: myapp
    environment: production
spec:
  replicas: 3
  selector:
    matchLabels:
      app: myapp
  template:
    metadata:
      labels:
        app: myapp
    spec:
      containers:
      - name: myapp
        image: myapp:1.0.0
        ports:
        - containerPort: 8080
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-secret
              key: url
        - name: LOG_LEVEL
          value: "info"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
```

## SQL

```sql
-- Complex SQL query with joins and aggregations
SELECT 
    u.user_id,
    u.username,
    u.email,
    COUNT(o.order_id) AS total_orders,
    COALESCE(SUM(o.total_amount), 0) AS total_spent,
    MAX(o.order_date) AS last_order_date,
    CASE 
        WHEN COUNT(o.order_id) > 10 THEN 'VIP'
        WHEN COUNT(o.order_id) > 5 THEN 'Regular'
        ELSE 'New'
    END AS customer_tier
FROM 
    users u
LEFT JOIN 
    orders o ON u.user_id = o.user_id
WHERE 
    u.status = 'active'
    AND u.created_at >= '2023-01-01'
GROUP BY 
    u.user_id, u.username, u.email
HAVING 
    COUNT(o.order_id) > 0
ORDER BY 
    total_spent DESC,
    last_order_date DESC
LIMIT 100;
```

## Nested Code Blocks (Edge Case)

````markdown
This is a markdown file with nested code blocks:

```python
def example():
    print("This is Python code")
```

And more text after the code block.
````

## Multiple Code Blocks in Sequence

```python
print("First block")
```

```python
print("Second block")
```

```python
print("Third block")
```
