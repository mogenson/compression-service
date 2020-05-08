# Compression Service

By Michael Mogenson

## Table of Contents
1. [Target Platform](#target-platform)  
2. [Description](#description)  
    - [Implementer Defined Status Codes](#implementer-defined-status-codes)  
3. [Usage](#usage)  
4. [Libraries](#libraries)  
5. [Assumptions](#assumptions)  
6. [Improvements](#improvements)  
    - [Implementation](#implementation)  
    - [API](#api)  

## Target Platform

This project was developed on Arch Linux, but tested to work on Ubuntu 18.04 LTS x86_64 and a Raspberry Pi 3B+ running Raspbian Buster.

## Description

This compression service accepts packets on port 4000. An async executor is used to spawn an async task for each new client so that multiple simultaneous connections can be made to the same port.

A `PacketCodec` is created inside each task. The `PacketCodec` reads bytes from the socket as they arrive and parses the magic header, payload length, request code, and optionally, the payload. Since bytes may arrive from the socket in incomplete chunks, the `PacketCodec` scans the incoming bytes until a magic header is found, then begins parsing the remaining fields from that location. The `PacketCodec` returns either a `RequestCode` enum when a packet is successfully parsed, or a `StatusCode` enum if the packet is invalid or mis-formatted.

The returned `RequestCode` is processed and a `StatusCode` is generated and passed to the `PacketCodec`. The `PacketCodec` generates a response packet with the appropiate magic header, payload length, status code value, and optionally, a payload. It writes the response packet to an output buffer to be sent over the socket.

The `Compress` variant of the `RequestCode` enum contains a reference to the payload section of the received packet's buffer. This is passed to a `Compressor` prefix encoder. The referenced buffer is read from start to finish, and replaced inline with compressed data. No buffer copies are performed during packet parsing or payload compressing. While reading the payload buffer, we store the current letter and keep count of how many times it occurs in a row. When a different letter is read, we pass the stored letter and count to a label writing routine, which calculates whether the prefix label plus letter is shorter than the section of repeated letter. If so, the buffer is overwritten with the label and letter, if not the original sequence of letters is written. A read index keeps track of the buffer read position and a write index keeps track of the buffer write position. When the buffer is fully read and processed, a new and shorter reference to just the compressed section of the payload buffer is returned. This is placed into an `Ok` `StatusCode` and sent to the client.

Both `PacketCodec` and `Compressor` keep track of how many bytes they receive and how many bytes they send or process. After each request and response transaction, the async task collects the usage stats, unlocks a shared mutex to a global `Stats` structure, and updates the server stats. Local stats are cleared after every request and response transaction, and global stats are reset from a `ResetStats` `RequestCode`. A `GetStats` `RequestCode` returns the global stats plus any not-yet-updated local stats.

### Implementer Defined Status Codes

|Value|Description|
|-|-|
|33|Received a packet without a payload that requires one|
|34|Received a packet with a payload that should not have one|
|35|Payload contains non-ascii characters|
|36|Payload contains non-alphabetic characters|
|37|Payload contains non-lowercase characters|

## Usage

- Use `build.sh` to compile the project in release mode.
- Use `run.sh` to start the server in the foreground.
- Use `cargo test` to run the unit and integration tests.

## Libraries

The [Tokio](https://tokio.rs/) async runtime and [futures](https://rust-lang.github.io/futures-rs/) stream type were used to facilitate non-blocking IO and concurrent tasks. Creating a custom codec allowed the buffer processing and specific API values to be contained in the packet parsing. The main application logic could then be written around high level enum types. Tokio and the packet codec use the mutable bytes buffer type from [bytes](https://github.com/tokio-rs/bytes). The compressor also adopted this type to make interoperability easy.

## Assumptions

- If the magic header does not match the start of a packet, keep reading in case we're offset from the actual packet start. This means there's no such thing as a 'wrong header' error since parsing cannot begin until a valid header is found.
- If we start reading a packet with a non-zero payload field for a request code that should not contain a payload, don't digest the payload, send an error response then resume looking for the start of a new packet.
- Don't timeout or give up on reading the entire contents of a packet's payload. If a packet contained an incorrect payload length, we may read up to the max payload length number of bytes.
- A Get Stats request should return the combined statistics for every current and past client since the server began running or any client sent a Reset Stats request. There can be many simultaneous clients on the same port of the same server. Since each client does not synchronize stats until a complete request and response transaction, the stats requested by one client can be slightly off based on the transaction state of the other clients at that exact point in time.
- The total bytes received value of a Get Stats request should include the length of the the request packet that was just received and parsed.
- The total bytes sent value of a Get Stats request should not include the length of the response packet that is just about to be sent.
- All received bytes should be counted for stats, including those that were part of invalid or partially parsed packets.
- Only payloads that were entirely valid, completely compressed, and returned to clients should be used to calculate the compression ratio stat.

## Improvements

### Implementation

- Log events and errors to either a local log file or a system level loging framework like 'journald'.
- Provide command line flags or a configuration file to specify runtime options such as: max payload length, what port to use, and max number of simultaneous clients.
- Integrate with a system level service manager like 'systemd' or network hook like 'dhcpcd' to start automatically and restart in case of failure.
- Use an encrypted protocol such as WSS or QUIC for data security and privacy.
- Improve performance by compressing payload chunks inline as they arrive, instead of waiting for a complete payload.

### API

- Swap the payload length and request code fields. Requests and responses that do not use a payload could reduce packet size by omitting the payload length field.
- Choose a magic header that does not overlap with the ascii letter range. This would ensure there is no chance of finding a magic header in the middle of an ascii payload.
- Include a checksum in the header or at the end of a payload. This would ensure that a packet was correctly parsed, and that the payload length and request code values could be trusted.
