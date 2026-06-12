#!/bin/bash

echo "🔍 开始查找并删除所有子目录中的编译垃圾"
echo "----------------------------------------"

# 删除所有 target 目录
find . -type d -name "target" | while read -r dir; do
    echo "✅ 已删除目录：$dir"
    rm -rf "$dir"
done

# 删除所有 .DS_Store 文件
find . -type f -name ".DS_Store" | while read -r file; do
    echo "✅ 已删除文件：$file"
    rm -f "$file"
done

# 删除所有 .git 目录
find . -type d -name ".git" | while read -r dir; do
    echo "✅ 已删除目录：$dir"
    rm -rf "$dir"
done

# 删除所有 .gitignore 文件
find . -type f -name ".gitignore" | while read -r file; do
    echo "✅ 已删除文件：$file"
    rm -f "$file"
done

# 删除所有 Cargo.lock 文件
find . -type f -name "Cargo.lock" | while read -r file; do
    echo "✅ 已删除文件：$file"
    rm -f "$file"
done

echo "----------------------------------------"
echo "🎉 全部删除完成！"