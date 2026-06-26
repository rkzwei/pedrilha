#!/usr/bin/env bash
set -e
INSTALL_DIR=/opt/gem-finder

# Create user
useradd --system --no-create-home --shell /bin/false gem-finder 2>/dev/null || true

# Create install dir
mkdir -p $INSTALL_DIR
cp target/release/gem-finder-api $INSTALL_DIR/
cp -r dist/ $INSTALL_DIR/dist/
cp deploy/gem-finder.service /etc/systemd/system/
cp SECRETS.env $INSTALL_DIR/.env  # user must create this first

chown -R gem-finder:gem-finder $INSTALL_DIR
chmod 600 $INSTALL_DIR/.env

systemctl daemon-reload
systemctl enable gem-finder
systemctl start gem-finder
echo "gem-finder started. Check: systemctl status gem-finder"
