# Rust WebSocket Server

A high-performance WebSocket server implemented in Rust that supports topic-based pub/sub messaging with password protection and heartbeat monitoring.

## Features

- Topic-based publish/subscribe messaging
- Password-protected topics
- Automatic client heartbeat monitoring (30s timeout)
- JSON message format
- Status codes for operation results (200, 401, 403, 404, etc.)
- System commands via `/system` topic

## Installation

1. Install Rust if you haven't already: 
```bash 
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. Clone the repository: 
```bash
git clone https://github.com/jrivers-iclass/rust-ws.git
cd rust-ws
```

3. Build and run:
```bash
cargo run
```


The server will start on `ws://localhost:8000/ws`

## Message Format

All messages use the following JSON format:

```json
{
"action": "subscribe|publish|create",
"topic": "topic-name",
"message": "your message here",
"password": "optional-password"
}
```


## Available Actions

### 1. Create Topic
Creates a new topic, optionally password-protected.
```json
{
"action": "create",
"topic": "private-room",
"message": "",
"password": "optional-password"
}
```

Response:
- 200: Topic created successfully
- 409: Topic already exists

### 2. Subscribe to Topic
Join a topic to receive messages.
```json
{
"action": "subscribe",
"topic": "private-room",
"message": "",
"password": "required-if-protected"
}
```

Response:
- 200: Subscribed to topic successfully
- 401: Incorrect password
- 404: Topic does not exist

### 3. Publish to Topic
Send a message to all subscribers of a topic.
```json
{
"action": "publish",
"topic": "private-room",
"message": "Hello everyone!",
"password": null
}
```

Response:
- 200: Message sent successfully
- 403: Not subscribed to topic
- 404: Topic does not exist

## System Commands

Send these messages to the "system" topic:
```json
{
"action": "publish",
"topic": "system",
"message": "/command",
"password": null
}
```

Available commands:
- `/help` - Show available commands
- `/verbose` - Toggle verbose logging

## Heartbeat

## Example Client (JavaScript)
```javascript
const ws = new WebSocket('ws://localhost:8000/ws');
ws.onopen = () => {
    // Subscribe to a topic
    ws.send(JSON.stringify({
        action: "subscribe",
        topic: "chat",
        message: "",
        password: null
    }));
};
ws.onmessage = (event) => {
const response = JSON.parse(event.data);
    console.log('Received:', response);
};
// Publish a message
function sendMessage(message) {
    ws.send(JSON.stringify({
        action: "publish",
        topic: "chat",
        message: message,
        password: null
    }));
}
```

## Error Codes

- 200: Success
- 400: Bad Request (unknown action)
- 401: Unauthorized (wrong password)
- 403: Forbidden (not subscribed)
- 404: Not Found (topic doesn't exist)
- 409: Conflict (topic already exists)
- 500: Server Error