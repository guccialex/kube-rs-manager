FROM rust

WORKDIR /home
COPY . .

#RUN cargo install --path .
RUN rustup update nightly
RUN rustup default nightly


CMD cargo run