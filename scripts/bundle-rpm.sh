#!/usr/bin/env bash
#
# BananaTray RPM 打包脚本
#
# 使用方法:
#   bash scripts/bundle-rpm.sh              # 默认 release 构建
#   bash scripts/bundle-rpm.sh --skip-build  # 跳过编译
#   bash scripts/bundle-rpm.sh --arch x86_64 # 指定架构
#
# 前置要求:
#   - rpmbuild (rpm-build 包)
#     Fedora/RHEL: sudo dnf install rpm-build
#     Ubuntu:      sudo apt install rpm
#
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

init_project_vars
parse_args "$@"

# RPM 使用的架构名与 deb 不同
RPM_ARCH="${ARCH}"
if [ "$RPM_ARCH" = "amd64" ]; then
    RPM_ARCH="x86_64"
elif [ "$RPM_ARCH" = "arm64" ]; then
    RPM_ARCH="aarch64"
fi

ensure_build

DESCRIPTION="System tray application for monitoring AI coding assistant quotas"

RPM_TOPDIR="$BUNDLE_DIR/rpmbuild"
SPEC_FILE="$RPM_TOPDIR/SPECS/$APP_NAME.spec"
BUILDROOT="$RPM_TOPDIR/BUILDROOT/${APP_NAME}-${VERSION}-1.${RPM_ARCH}"

# ------------------------------------------------------------------
# 1. 组装 BUILDROOT
# ------------------------------------------------------------------
echo "📦 组装 rpmbuild 目录结构..."
rm -rf "$RPM_TOPDIR"
mkdir -p "$RPM_TOPDIR"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

# 标准安装树 (usr/bin, desktop, icons, metainfo, resources)
assemble_install_tree "$BUILDROOT"

# ------------------------------------------------------------------
# 2. 生成 .spec 文件
# ------------------------------------------------------------------
echo "📝 生成 RPM spec 文件..."

# spec 中 %install 为空：文件已预装在 BUILDROOT 中，通过 --buildroot 传入
cat > "$SPEC_FILE" <<SPEC_EOF
Name:           $APP_NAME
Version:        $VERSION
Release:        1%{?dist}
Summary:        $DESCRIPTION
License:        MIT
URL:            $HOMEPAGE_URL

AutoReqProv:    no
Requires:       gtk3
Requires:       libayatana-appindicator-gtk3

%description
BananaTray is a cross-platform system tray application for monitoring
AI coding assistant quotas. It supports multiple AI providers including
Claude, Gemini, GitHub Copilot, Codex, and more.

# 文件已预装在 BUILDROOT 中，无需额外安装步骤
%install

%files
%{_bindir}/$APP_NAME
%{_datadir}/applications/$APP_NAME.desktop
%{_datadir}/icons/hicolor/*/apps/$APP_NAME.png
%{_datadir}/metainfo/com.bananatray.app.metainfo.xml
%{_datadir}/$APP_NAME/

%post
update-desktop-database %{_datadir}/applications &>/dev/null || :
touch --no-create %{_datadir}/icons/hicolor &>/dev/null || :
gtk-update-icon-cache %{_datadir}/icons/hicolor &>/dev/null || :

%postun
update-desktop-database %{_datadir}/applications &>/dev/null || :
touch --no-create %{_datadir}/icons/hicolor &>/dev/null || :
gtk-update-icon-cache %{_datadir}/icons/hicolor &>/dev/null || :
SPEC_EOF

# ------------------------------------------------------------------
# 3. 构建 .rpm 包
# ------------------------------------------------------------------
echo "📦 构建 .rpm 包..."

if command -v rpmbuild &>/dev/null; then
    # --buildroot 显式指定预装目录，避免 spec 内使用脆弱的相对路径
    rpmbuild --define "_topdir $RPM_TOPDIR" \
             --buildroot "$BUILDROOT" \
             --target "$RPM_ARCH" \
             -bb "$SPEC_FILE"

    RPM_FILE=$(find "$RPM_TOPDIR/RPMS" -name "*.rpm" -type f | head -1)
    if [ -n "$RPM_FILE" ]; then
        cp "$RPM_FILE" "$BUNDLE_DIR/"
        RPM_BASENAME=$(basename "$RPM_FILE")
        echo "✅ .rpm 包已创建: $BUNDLE_DIR/$RPM_BASENAME"
        echo ""
        echo "📋 包信息:"
        rpm -qip "$BUNDLE_DIR/$RPM_BASENAME" 2>/dev/null || true
        echo ""
        echo "🚀 安装: sudo dnf install ./$RPM_BASENAME"
    else
        echo "❌ rpmbuild 完成但未找到 .rpm 文件"
        exit 1
    fi
else
    echo "⚠️  rpmbuild 不可用"
    echo "   Fedora/RHEL: sudo dnf install rpm-build"
    echo "   Ubuntu:      sudo apt install rpm"
    echo ""
    echo "   spec 文件已生成: $SPEC_FILE"
    echo "   BUILDROOT 已组装: $BUILDROOT"
fi
