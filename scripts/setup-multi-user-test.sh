#!/bin/bash
# setup-multi-user-test.sh — 一键搭建 cowiki 多人权限测试场景
# 用法: bash scripts/setup-multi-user-test.sh

set -e
API="${COWIKI_API:-http://localhost:3000/api}"
TIMESTAMP=$(date +%s)

echo "=== 1. 注册用户 ==="
ALICE=$(curl -s -X POST "$API/auth/register" -H 'Content-Type: application/json' -d "{\"name\":\"Alice-$TIMESTAMP\"}")
BOB=$(curl -s -X POST "$API/auth/register" -H 'Content-Type: application/json' -d "{\"name\":\"Bob-$TIMESTAMP\"}")
CAROL=$(curl -s -X POST "$API/auth/register" -H 'Content-Type: application/json' -d "{\"name\":\"Carol-$TIMESTAMP\"}")

ALICE_KEY=$(echo "$ALICE" | python3 -c "import sys,json;print(json.load(sys.stdin)['api_key'])")
BOB_KEY=$(echo "$BOB" | python3 -c "import sys,json;print(json.load(sys.stdin)['api_key'])")
CAROL_KEY=$(echo "$CAROL" | python3 -c "import sys,json;print(json.load(sys.stdin)['api_key'])")
ALICE_ID=$(echo "$ALICE" | python3 -c "import sys,json;print(json.load(sys.stdin)['user']['id'])")
BOB_ID=$(echo "$BOB" | python3 -c "import sys,json;print(json.load(sys.stdin)['user']['id'])")
CAROL_ID=$(echo "$CAROL" | python3 -c "import sys,json;print(json.load(sys.stdin)['user']['id'])")

echo "  Alice: $ALICE_ID"
echo "  Bob:   $BOB_ID"
echo "  Carol: $CAROL_ID"

echo ""
echo "=== 2. 设置 email ==="
sudo docker exec cowiki-db-1 psql -U cowiki -d cowiki -c "
  UPDATE users SET email='alice@test.com' WHERE id='$ALICE_ID';
  UPDATE users SET email='bob@test.com' WHERE id='$BOB_ID';
  UPDATE users SET email='carol@test.com' WHERE id='$CAROL_ID';
" 2>/dev/null || echo "  (跳过 — 可能不需要 sudo 或无 docker)"

echo ""
echo "=== 3. 创建 workspace ==="
SLUG="team-$TIMESTAMP"
curl -s -X POST "$API/workspaces" -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d "{\"name\":\"Test Team\",\"slug\":\"$SLUG\",\"visibility\":\"public\"}" | python3 -c "import sys,json;d=json.load(sys.stdin);print(f'  Workspace: {d[\"slug\"]} ({d[\"role\"]})')"

echo ""
echo "=== 4. Alice 邀请 Bob(writer) + Carol(reader) ==="
curl -s -X POST "$API/workspaces/$SLUG/invite" -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d '{"email":"bob@test.com","role":"writer"}' | python3 -c "import sys,json;d=json.load(sys.stdin);print(f'  Bob invited: {d[\"invitation_id\"][:8]}...')"

curl -s -X POST "$API/workspaces/$SLUG/invite" -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d '{"email":"carol@test.com","role":"reader"}' | python3 -c "import sys,json;d=json.load(sys.stdin);print(f'  Carol invited: {d[\"invitation_id\"][:8]}...')"

echo ""
echo "=== 5. 验证 ==="
PENDING=$(curl -s "$API/invitations/pending" -H "Authorization: Bearer $BOB_KEY")
COUNT=$(echo "$PENDING" | python3 -c "import sys,json;print(len(json.load(sys.stdin)))")
echo "  Bob pending: $COUNT"

echo ""
echo "========================"
echo "Alice (owner):"
echo "  localStorage.setItem('cowiki_api_key','$ALICE_KEY')"
echo "  localStorage.setItem('cowiki_user',JSON.stringify({id:'$ALICE_ID',name:'Alice'}))"
echo "  location.reload()"
echo ""
echo "Bob (writer — 无痕窗口):"
echo "  localStorage.setItem('cowiki_api_key','$BOB_KEY')"
echo "  localStorage.setItem('cowiki_user',JSON.stringify({id:'$BOB_ID',name:'Bob'}))"
echo "  location.reload()"
echo ""
echo "Carol (reader — 第二个无痕窗口):"
echo "  localStorage.setItem('cowiki_api_key','$CAROL_KEY')"
echo "  localStorage.setItem('cowiki_user',JSON.stringify({id:'$CAROL_ID',name:'Carol'}))"
echo "  location.reload()"
