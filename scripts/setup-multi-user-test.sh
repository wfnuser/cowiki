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
DAVE=$(curl -s -X POST "$API/auth/register" -H 'Content-Type: application/json' -d "{\"name\":\"Dave-$TIMESTAMP\"}")

ALICE_KEY=$(echo "$ALICE" | python3 -c "import sys,json;print(json.load(sys.stdin)['api_key'])")
BOB_KEY=$(echo "$BOB" | python3 -c "import sys,json;print(json.load(sys.stdin)['api_key'])")
CAROL_KEY=$(echo "$CAROL" | python3 -c "import sys,json;print(json.load(sys.stdin)['api_key'])")
DAVE_KEY=$(echo "$DAVE" | python3 -c "import sys,json;print(json.load(sys.stdin)['api_key'])")
ALICE_ID=$(echo "$ALICE" | python3 -c "import sys,json;print(json.load(sys.stdin)['user']['id'])")
BOB_ID=$(echo "$BOB" | python3 -c "import sys,json;print(json.load(sys.stdin)['user']['id'])")
CAROL_ID=$(echo "$CAROL" | python3 -c "import sys,json;print(json.load(sys.stdin)['user']['id'])")
DAVE_ID=$(echo "$DAVE" | python3 -c "import sys,json;print(json.load(sys.stdin)['user']['id'])")

echo "  Alice: $ALICE_ID (owner)"
echo "  Bob:   $BOB_ID (manager)"
echo "  Carol: $CAROL_ID (editor)"
echo "  Dave:  $DAVE_ID (viewer)"

echo ""
echo "=== 2. 设置 email ==="
sudo docker exec cowiki-db-1 psql -U cowiki -d cowiki -c "
  UPDATE users SET email='alice@test.com' WHERE id='$ALICE_ID';
  UPDATE users SET email='bob@test.com' WHERE id='$BOB_ID';
  UPDATE users SET email='carol@test.com' WHERE id='$CAROL_ID';
  UPDATE users SET email='dave@test.com' WHERE id='$DAVE_ID';
" 2>/dev/null || echo "  (跳过 — 可能不需要 sudo 或无 docker)"

echo ""
echo "=== 3. Alice 创建 workspace ==="
SLUG="team-$TIMESTAMP"
curl -s -X POST "$API/workspaces" -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d "{\"name\":\"Test Team\",\"slug\":\"$SLUG\",\"visibility\":\"public\"}" | python3 -c "import sys,json;d=json.load(sys.stdin);print(f'  Workspace: {d[\"slug\"]} ({d[\"role\"]})')"

echo ""
echo "=== 4. Alice 邀请 Bob(manager) + Carol(editor) + Dave(viewer) ==="
# 使用 UUID 邀请 (最可靠的方式, 不依赖 email 设置)
# resolve_user_identifier 支持 UUID → email → username 三级回退
BOB_INVITE=$(curl -s -X POST "$API/workspaces/$SLUG/invite" -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d "{\"invitations\":[{\"user\":\"$BOB_ID\",\"role\":\"manager\"}]}")
echo "$BOB_INVITE" | python3 -c "import sys,json;d=json.load(sys.stdin);r=d['results'][0];rid=r.get('invitation_id','');print(f'  Bob (id)→ manager: {r[\"status\"]} ({rid[:8] if rid else r.get(\"reason\",\"\")})')"

CAROL_INVITE=$(curl -s -X POST "$API/workspaces/$SLUG/invite" -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d "{\"invitations\":[{\"user\":\"$CAROL_ID\",\"role\":\"editor\"}]}")
echo "$CAROL_INVITE" | python3 -c "import sys,json;d=json.load(sys.stdin);r=d['results'][0];rid=r.get('invitation_id','');print(f'  Carol (id)→ editor: {r[\"status\"]} ({rid[:8] if rid else r.get(\"reason\",\"\")})')"

DAVE_INVITE=$(curl -s -X POST "$API/workspaces/$SLUG/invite" -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ALICE_KEY" \
  -d "{\"invitations\":[{\"user\":\"$DAVE_ID\",\"role\":\"viewer\"}]}")
echo "$DAVE_INVITE" | python3 -c "import sys,json;d=json.load(sys.stdin);r=d['results'][0];rid=r.get('invitation_id','');print(f'  Dave (id)→ viewer: {r[\"status\"]} ({rid[:8] if rid else r.get(\"reason\",\"\")})')"

# echo ""
# echo "=== 5. Bob / Carol / Dave 接受邀请 ==="
# # 获取各自的 pending invitations 并接受
# BOB_INV=$(curl -s "$API/invitations/pending" -H "Authorization: Bearer $BOB_KEY" | python3 -c "import sys,json;d=json.load(sys.stdin);print(d[0]['id'] if d else '')")
# CAROL_INV=$(curl -s "$API/invitations/pending" -H "Authorization: Bearer $CAROL_KEY" | python3 -c "import sys,json;d=json.load(sys.stdin);print(d[0]['id'] if d else '')")
# DAVE_INV=$(curl -s "$API/invitations/pending" -H "Authorization: Bearer $DAVE_KEY" | python3 -c "import sys,json;d=json.load(sys.stdin);print(d[0]['id'] if d else '')")
#
# if [ -n "$BOB_INV" ]; then
#   curl -s -X POST "$API/invitations/$BOB_INV/accept" -H "Authorization: Bearer $BOB_KEY" | python3 -c "import sys,json;d=json.load(sys.stdin);print(f'  Bob accepted → role: {d[\"role\"]} in {d[\"slug\"]}')"
# fi
# if [ -n "$CAROL_INV" ]; then
#   curl -s -X POST "$API/invitations/$CAROL_INV/accept" -H "Authorization: Bearer $CAROL_KEY" | python3 -c "import sys,json;d=json.load(sys.stdin);print(f'  Carol accepted → role: {d[\"role\"]} in {d[\"slug\"]}')"
# fi
# if [ -n "$DAVE_INV" ]; then
#   curl -s -X POST "$API/invitations/$DAVE_INV/accept" -H "Authorization: Bearer $DAVE_KEY" | python3 -c "import sys,json;d=json.load(sys.stdin);print(f'  Dave accepted → role: {d[\"role\"]} in {d[\"slug\"]}')"
# fi
#
# echo ""
# echo "=== 6. 验证成员列表 ==="
# curl -s "$API/workspaces/$SLUG/members" -H "Authorization: Bearer $ALICE_KEY" | python3 -c "
# import sys,json
# members = json.load(sys.stdin)
# for m in members:
#     print(f'  {m[\"name\"]:12s} → {m[\"role\"]:10s}  email={m.get(\"email\",\"n/a\")}')"

echo ""
echo "========================"
echo "  Workspace: $SLUG"
echo ""
echo "Alice (owner):"
echo "  localStorage.setItem('cowiki_api_key','$ALICE_KEY')"
echo "  localStorage.setItem('cowiki_user',JSON.stringify({id:'$ALICE_ID',name:'Alice'}))"
echo "  location.reload()"
echo ""
echo "Bob (manager — 无痕窗口):"
echo "  localStorage.setItem('cowiki_api_key','$BOB_KEY')"
echo "  localStorage.setItem('cowiki_user',JSON.stringify({id:'$BOB_ID',name:'Bob'}))"
echo "  location.reload()"
echo ""
echo "Carol (editor — 第二个无痕窗口):"
echo "  localStorage.setItem('cowiki_api_key','$CAROL_KEY')"
echo "  localStorage.setItem('cowiki_user',JSON.stringify({id:'$CAROL_ID',name:'Carol'}))"
echo "  location.reload()"
echo ""
echo "Dave (viewer — 第三个无痕窗口):"
echo "  localStorage.setItem('cowiki_api_key','$DAVE_KEY')"
echo "  localStorage.setItem('cowiki_user',JSON.stringify({id:'$DAVE_ID',name:'Dave'}))"
echo "  location.reload()"
