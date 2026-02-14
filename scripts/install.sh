#!/bin/bash
#
# VoiceTerm Installer (overlay mode)
# Run: ./scripts/install.sh
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/setup.sh" install "$@"
