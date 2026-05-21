#!/usr/bin/env bash
# Wide-char + emoji column alignment. Each row should land flush at col 0.
set -eu
echo "中文测试 — 宽字符占两格"
echo "日本語テスト — ひらがな + 漢字"
echo "한국어 테스트 — 한글 자모"
echo "Emoji: 🐱 🚀 ❤️ 🇨🇳 (regional indicator pair)"
echo "Mixed: hello 你好 こんにちは 🌍 done"
# Alignment check — vertical bars should line up under each other.
printf '|%-10s|\n' "ASCII"
printf '|%-10s|\n' "你好"
printf '|%-10s|\n' "🚀"
