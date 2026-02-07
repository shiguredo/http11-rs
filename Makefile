.PHONY: test cover pbt pbt-cover fuzz fuzzing fuzzing-parallel fuzzing-list check clippy fmt clean

# 全テストを実行する
test:
	cargo test --workspace

# 全テストカバレッジ付きで実行する
cover:
	cargo llvm-cov --tests --workspace

# PBT をカバレッジ付きで実行する
pbt-with-cover:
	cargo llvm-cov -p pbt --tests

# Fuzzing を全ターゲットで逐次実行する（fork 数はコア数に応じて自動調整）
fuzzing:
	@FORKS=$$(( $$(nproc) - 2 )); \
	if [ $$FORKS -lt 1 ]; then FORKS=1; fi; \
	echo "Using fork=$$FORKS on $$(nproc) cores"; \
	for target in $$(cargo fuzz list); do \
		echo "=== Fuzzing $$target ==="; \
		cargo +nightly fuzz run $$target -- -max_total_time=30 -fork=$$FORKS -max_len=4096 || exit 1; \
	done

# Fuzzing を全ターゲットで並列実行しレポートを出力する（fork 数はコア数に応じて自動調整）
fuzzing-parallel:
	@mkdir -p fuzz/logs
	@FORKS=$$(( $$(nproc) - 2 )); \
	if [ $$FORKS -lt 1 ]; then FORKS=1; fi; \
	echo "Using fork=$$FORKS on $$(nproc) cores"; \
	cargo fuzz list | xargs -P $$(cargo fuzz list | wc -l) -I {} \
		sh -c 'cargo +nightly fuzz run {} -- -max_total_time=30 -fork=1 -max_len=4096 > fuzz/logs/{}.log 2>&1'
	@echo "=== Fuzzing Report ==="
	@for f in fuzz/logs/*.log; do \
		target=$$(basename $$f .log); \
		last=$$(grep -E '^#[0-9]+:' $$f | tail -1); \
		echo "$$target: $$last"; \
	done

# Fuzzing ターゲット一覧を表示する
fuzzing-list:
	cargo fuzz list

# cargo check を実行する
check:
	cargo check --workspace

# cargo clippy を実行する
clippy:
	cargo clippy --workspace -- -D warnings

# cargo fmt を実行する
fmt:
	cargo fmt --all

# ビルド成果物を削除する
clean:
	cargo clean
