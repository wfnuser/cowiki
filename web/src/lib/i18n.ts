/**
 * App language preference (`en` or `zh-CN`), stored per device alongside the
 * theme preference. English strings double as keys so components stay readable
 * and TypeScript catches missing Chinese translations at compile time.
 */

export type Locale = 'en' | 'zh-CN';

export const LOCALE_STORAGE_KEY = 'cowiki.locale';

export const LOCALE_CHOICES: ReadonlyArray<[Locale, string]> = [
  ['en', 'English'],
  ['zh-CN', '中文'],
];

type StorageLike = Pick<globalThis.Storage, 'getItem' | 'setItem'>;

export function loadLocale(storage: StorageLike | undefined = globalThis.localStorage): Locale {
  try {
    const value = storage?.getItem(LOCALE_STORAGE_KEY);
    return value === 'zh-CN' ? 'zh-CN' : 'en';
  } catch {
    return 'en';
  }
}

export function saveLocale(locale: Locale, storage: StorageLike | undefined = globalThis.localStorage): void {
  try {
    storage?.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // Private windows can refuse storage; the choice still applies now.
  }
}

export function applyLocale(locale: Locale): void {
  const root = globalThis.document?.documentElement;
  if (root) {
    root.lang = locale;
    root.dataset.locale = locale;
  }
  globalThis.dispatchEvent?.(new CustomEvent('cowiki:locale', { detail: locale }));
}

export function startLocale(): Locale {
  const locale = loadLocale();
  applyLocale(locale);
  return locale;
}

export function setLocalePreference(locale: Locale): void {
  saveLocale(locale);
  applyLocale(locale);
}

export const ZH_CN = {
  // Navigation & shell
  'Home': '首页',
  'Journals': '日记',
  'Tasks': '任务',
  'Flashcards': '闪卡',
  'Reviews': '评审',
  'History': '历史',
  'Members': '成员',
  'Members & roles': '成员与权限',
  'Broken links': '断链',
  'Notifications': '通知',
  'Discover': '发现',
  'Join a Space': '加入空间',
  'Settings': '设置',
  'Sources': '来源',
  'Knowledge': '知识库',
  'Search...': '搜索…',
  'Search…': '搜索…',
  'No sources yet': '暂无来源',
  'No pages yet': '暂无页面',
  'Add Source': '添加来源',
  'Add page or folder': '新建页面或文件夹',
  'New Page': '新建页面',
  'New Folder': '新建文件夹',
  'Select a space': '选择一个空间',
  'Add a Space': '添加空间',
  'Back': '后退',
  'Forward': '前进',
  'Collapse sidebar': '收起侧边栏',
  'Expand sidebar': '展开侧边栏',
  'Show sidebar': '展开侧边栏',
  'New page': '新建页面',
  'New folder': '新建文件夹',
  'Add source': '添加来源',
  'Rename': '重命名',
  'Delete': '删除',
  'Delete folder': '删除文件夹',
  'Sign in': '登录',
  'Sign out': '退出登录',
  'Sign up / Sign in': '注册 / 登录',
  'Sign up or sign in': '注册或登录',
  'Not signed in': '未登录',
  'Comments': '评论',
  'Back to reading and show comments': '返回阅读并打开评论',
  'Agent': '智能体',
  'Open Agent': '打开智能体',
  'Open the Agent sidebar': '打开智能体侧边栏',
  'Close Agent': '关闭智能体',
  'Source File': '源文件',
  'View original': '查看原文',
  'This page does not exist in the selected version.': '此页面在所选版本中不存在。',
  'Viewing upstream “main”': '正在查看上游 “main” 版本',
  'Viewing {title} changes': '正在查看「{title}」的修改',

  // Space status & versions
  'Draft': '草稿',
  'Current Draft': '当前草稿',
  'Your working copy': '你的工作副本',
  'Latest commit · read-only': '最新提交 · 只读',
  'Version': '版本',
  'See all in Reviews': '在评审中查看全部',
  'Agent change': '智能体修改',
  'Local Space': '本地空间',
  'Cloud': '云端',
  'Shared Cloud Space': '共享云端空间',
  'Local only': '仅本地',
  'Changes stay on this device until you publish': '在发布之前，所有修改仅保存在本机',
  'Local · Checking link': '本地 · 正在检查连接',
  'This Space is stored on this device': '此空间存储在本机',
  'Local · Link unavailable': '本地 · 无法读取连接状态',
  'CoWiki could not read this Space’s Cloud link status': 'CoWiki 无法读取此空间的云端连接状态',
  'Local + Cloud · Not uploaded': '本地 + 云端 · 尚未上传',
  'Local changes have not been submitted to Cloud': '本地改动尚未提交到云端',
  'Local + Cloud · In review': '本地 + 云端 · 评审中',
  'Your Cloud submission is waiting for Review': '你的云端提交正在等待评审',
  'Local + Cloud · Cloud update': '本地 + 云端 · 有云端更新',
  'Cloud has changes that are not in this local copy': '云端存在尚未同步到本机的修改',
  'Local + Cloud · Attention': '本地 + 云端 · 需要处理',
  'Cloud sync needs your attention': '云端同步需要你的处理',
  'Local + Cloud · Synced': '本地 + 云端 · 已同步',
  'This local copy is linked to Cloud': '此本地副本已与云端关联',
  'Sign in to share…': '登录后分享…',
  'Cloud sync…': '云端同步…',
  'Publish to Cloud…': '发布到云端…',
  'Publish to CoWiki Cloud': '发布到 CoWiki 云端',
  'Sync with CoWiki Cloud': '与 CoWiki 云端同步',
  'Sign in to publish': '登录后发布',

  // Settings
  'App': '应用',
  'Space': '空间',
  'General': '通用',
  'Account': '账号',
  'API Keys': 'API 密钥',
  'Appearance': '外观',
  'System': '跟随系统',
  'Light': '浅色',
  'Dark': '深色',
  'Language': '语言',
  'Default Agent': '默认智能体',
  'New sessions start with it; the last one you use becomes the default.': '新建会话将默认使用此智能体；你上次使用的会自动成为默认项。',
  'Save checkpoints automatically': '自动保存快照',
  'After 5 quiet minutes and when CoWiki quits. Kept in History on this computer.': '静置 5 分钟后或退出 CoWiki 时自动记录，仅保存在本机历史中。',
  'Modules': '功能模块',
  'Turning a module off only hides it; your notes stay.': '关闭模块只会隐藏入口，你的笔记文件不会丢失。',
  'Daily journal': '每日日记',
  'One page a day; today first, earlier days as you scroll.': '每天一页，今日置顶，向下滚动浏览往日记录。',
  'Checkboxes from every page, sorted by day.': '汇集各页面中的待办复选框，按日期归类。',
  'List items tagged #card, reviewed like Anki. Progress stays on this device.': '带 #card 标签的列表项按 Anki 方式复习，进度保存在本机。',
  'Journal': '日记',
  'Folder': '文件夹',
  'Space root': '空间根目录',
  'Date format': '日期格式',
  'Template page': '模板页面',
  'Optional, e.g. Templates/Daily.md': '可选，例如 Templates/Daily.md',
  'Today’s note:': '今日笔记：',
  'Dates use Obsidian (moment) tokens such as YYYY-MM-DD. Templates support {{date}}, {{title}} and {{time}}.': '日期使用 Obsidian（moment）格式，如 YYYY-MM-DD。模板支持 {{date}}、{{title}} 和 {{time}}。',
  '{label} is off for this Space.': '此空间已关闭「{label}」。',
  'Turn on': '开启',
  'Templates': '模板',
  'Pages in this folder are offered when you create a page. They support {{title}}, {{date}} and {{time}}.': '新建页面时会列出此文件夹中的模板，支持 {{title}}、{{date}} 和 {{time}}。',
  'Loading Space settings…': '正在加载空间设置…',
  'Applies to everyone in {name}.': '适用于「{name}」中的所有成员。',
  'Only for you, on this computer.': '仅对你生效，保存在本机。',
  'Reload settings': '重新加载设置',
  'Unsaved changes in {name}': '「{name}」有未保存的更改',
  'Unsaved changes': '有未保存的更改',
  'Discard': '放弃',
  'Save': '保存',
  'Saving…': '正在保存…',
  'Reload': '重新加载',
  'Reloading…': '正在重新加载…',
  'Name': '名称',
  'User ID': '用户 ID',
  'Connect to CoWiki Cloud': '连接 CoWiki 云端',
  'Keep local Spaces account-free. Sign in only when you want to publish or join a shared Space.': '本地空间无需账号，仅在发布或加入协作空间时登录即可。',
  'Sign in with GitHub': '使用 GitHub 登录',
  'Your API Keys': '你的 API 密钥',
  'New Key': '新建密钥',
  'API keys are only shown once upon creation. Copy and store them securely.': 'API 密钥仅在创建时显示一次，请妥善复制并保存。',
  'Create your first API key to get started.': '创建第一个 API 密钥以开始使用。',
  'Create Key': '创建密钥',
  'Create API Key': '创建 API 密钥',
  'Key name': '密钥名称',
  'No API keys yet': '暂无 API 密钥',
  'Create a key to connect the CLI, MCP, or external agents.': '创建密钥以连接 CLI、MCP 或外部智能体。',
  'Revoke API Key': '吊销 API 密钥',
  'Revoke': '吊销',
  'Revoking...': '正在吊销…',
  'Create': '创建',
  'Creating...': '正在创建…',
  'Done': '已完成',
  'Cancel': '取消',
  'Close': '关闭',
  'Retry': '重试',
  'Refresh': '刷新',
  'Loading…': '加载中…',

  // Home & Journals
  'Needs you': '待处理',
  'Recently edited': '最近编辑',
  'No pages yet.': '暂无页面。',
  'task due': '项到期任务',
  'tasks due': '项到期任务',
  'card to review': '张待复习闪卡',
  'cards to review': '张待复习闪卡',
  'change to review': '项待评审修改',
  'changes to review': '项待评审修改',
  'broken link': '个断链',
  'broken links': '个断链',
  'Go to a day': '跳转到日期',
  'Journal calendar': '日记日历',
  'Write in this day': '记录这一天',
  'Previous month': '上个月',
  'Next month': '下个月',
  'Clear': '清除',
  'Date': '日期',
  'Due date': '截止日期',

  // Tasks
  'Task lists': '任务视图',
  'Today': '今天',
  'This week': '本周',
  'Upcoming': '待办日程',
  'No date': '无日期',
  'Overdue': '已逾期',
  'To do': '待办',
  'Doing': '进行中',
  'Layout': '布局',
  'List': '列表',
  'Board': '看板',
  'Add a task': '添加任务',
  'New task': '新任务',
  'Add': '添加',
  'Adding…': '添加中…',
  'Collecting tasks…': '正在收集任务…',
  'Nothing due today.': '今天没有到期任务。',
  'Nothing this week.': '本周没有任务。',
  'Nothing finished yet.': '还没有已完成的任务。',
  'No tasks here.': '这里没有任务。',
  'Tomorrow': '明天',
  'Next week': '下周',
  'Yesterday': '昨天',
  '{count} days ago': '{count} 天前',
  'Start': '开始',
  'Move back to To do': '退回待办',

  // Flashcards
  'New': '新卡',
  'Decks': '卡组列表',
  'All decks': '全部卡组',
  'Cards': '卡片',
  'Learning': '学习中',
  'Mastered': '已掌握',
  'Suspended': '已暂停',
  'Resume': '恢复',
  'in {count} days': '{count} 天后',
  '{total} card: {new} new, {learning} learning, {mastered} mastered.': '共 {total} 张：{new} 张新卡，{learning} 张学习中，{mastered} 张已掌握。',
  '{total} cards: {new} new, {learning} learning, {mastered} mastered.': '共 {total} 张：{new} 张新卡，{learning} 张学习中，{mastered} 张已掌握。',
  '{count} suspended.': '{count} 张已暂停。',
  '{count} card to review today.': '今天有 {count} 张卡要复习。',
  '{count} cards to review today.': '今天有 {count} 张卡要复习。',
  '{count} to review': '{count} 张',
  'All done for today.': '今天的卡都复习完了。',
  'Study {name}': '复习 {name}',
  'Collapse {name}': '收起 {name}',
  'Expand {name}': '展开 {name}',
  '{count}d': '{count} 天',
  '{count}mo': '{count} 个月',
  '{count}y': '{count} 年',
  'Daily limits': '每日上限',
  'New cards': '每日新卡',
  'New cards per day': '每日新卡上限',
  'Card reviews': '每日复习',
  'Space bar': '空格',
  'Reviews per day': '每日复习上限',
  'Collecting cards…': '正在收集闪卡…',
  'No cards yet. Tag a list item with #card to make one.': '暂无闪卡。在列表项中添加 #card 即可创建。',
  '{count} left': '剩余 {count} 张',
  'Review {count}': '第 {count} 次复习',
  'Session progress': '学习进度',
  'Open note': '打开笔记',
  'Skip': '跳过',
  'Skip (N)': '跳过 (N)',
  'Suspend': '暂停卡片',
  'Suspend: stop showing this card until you resume it': '暂停：恢复前不再显示此卡片',
  'Reset': '重置进度',
  'Reset: review this card again from the start': '重置：将此卡片作为新卡重新学习',
  'End': '结束',
  'End session': '结束本次学习',
  'The answer changed since your last review.': '自上次复习以来，答案内容已发生变化。',
  'Show answer': '显示答案',
  'Show clozes': '显示填空',
  'Forgotten': '忘记了',
  'Took a while': '想了会儿',
  'Remembered': '记得',
  '{count} card': '{count} 张卡片',
  '{count} cards': '{count} 张卡片',
  'suspended': '已暂停',
  'reviewed now': '本次已复习',
  'changed since review': '复习后有改动',

  // Reviews & Diff
  'Could not load Reviews': '无法加载评审列表',
  'No reviews yet.': '暂无评审。',
  'No changes': '无更改',
  'No file changes.': '没有文件改动。',
  'No changes in this file': '此文件无改动',
  'Loading changes…': '正在加载改动…',
  'Commit draft': '提交草稿',
  'Committing…': '正在提交…',
  'Draft committed.': '草稿已提交。',
  'Commit these changes to the current branch': '将这些改动提交到当前分支',
  'Compared with the last commit': '与上次提交对比',
  'Continue with Agent': '交给智能体继续处理',
  'Merge into Draft': '合并到草稿',
  'Merging…': '正在合并…',
  'Discarding…': '正在放弃…',
  'Could not update this Review': '无法更新此评审',
  'Needs resolution': '需要解决冲突',
  'Merged': '已合并',
  'Discarded': '已放弃',
  'Open': '进行中',
  'Added': '新增',
  'Deleted': '已删除',
  'Source diff': '源码对比',
  'Rendered preview': '渲染预览',
  'Review display': '评审展示方式',
  'Rendered preview is available for text Markdown files.': '仅 Markdown 文本文件支持渲染预览。',
  'Before': '修改前',
  'After': '修改后',
  'Removed page': '已删除页面',
  'Edit': '编辑',
  'Save review fix': '保存评审修改',
  'Saves as a review fix on this submission — the diff below updates to include it.': '将作为本次提交的评审修正保存，下方差异会自动更新。',
  'Click to comment': '点击添加评论',
  'Comment': '评论',
  'Write a comment…': '写下评论…',
  'Write a comment...': '写下评论…',
  'Resolve': '标记已解决',
  'Unresolve': '重新打开',
  'Resolved': '已解决',
  'Reply': '回复',
  'Reply...': '回复…',

  // History
  'Save snapshot': '保存快照',
  'Snapshot saved.': '快照已保存。',
  'Save the current Draft as a named snapshot in History': '将当前草稿保存为历史快照（不移动分支）',
  'History is kept for local Spaces only.': '仅本地空间保留历史记录。',
  'Could not update history': '无法更新历史记录',
  'No checkpoints yet.': '暂无快照记录。',

  // Broken links
  'Links to pages that don’t exist yet.': '指向尚未创建页面的链接。',
  'Checking links…': '正在检查链接…',
  'Could not inspect links': '无法检查链接',
  'No broken links.': '没有断链。',
  'Needs attention': '需要处理',

  // Search
  'Search': '搜索',
  'Search pages…': '搜索页面…',
  'Search in {name}…': '在「{name}」中搜索…',
  'Clear search': '清空搜索',
  'Search pages by title or content in this space': '按标题或正文搜索当前空间中的页面',
  'Searching…': '正在搜索…',
  'No results for “{query}”': '未找到与“{query}”相关的结果',
  'Pages': '页面',
  'Keyword': '关键词匹配',
  'Semantic': '语义相关',
  'linked': '关联链接',
  'related': '语义相关',
  'select': '选择',
  'open': '打开',
  'close': '关闭',
  'No matching pages.': '没有找到匹配的页面。',
  'Type to search this Space.': '输入关键词以搜索当前空间。',

  // Dialogs & modals
  'Title': '标题',
  'Template': '模板',
  'Blank page': '空白页面',
  'e.g. Meeting Notes': '例如：会议纪要',
  'e.g. Research, Projects': '例如：研究、项目',
  'Rename Space': '重命名空间',
  'Rename Page': '重命名页面',
  'Rename Folder': '重命名文件夹',
  'Delete Page': '删除页面',
  'Delete Folder': '删除文件夹',
  'Delete Workspace': '删除空间',
  'Create a Space': '创建空间',
  'Create local Space': '创建本地空间',
  'Import a folder': '导入文件夹',
  'Space name': '空间名称',
  'Create inside': '创建位置',
  'Choose a parent folder…': '选择父文件夹…',
  'Choose an existing folder…': '选择已有文件夹…',
  'Creating…': '正在创建…',
  'Import': '导入',
  'Add your first Space': '添加你的第一个空间',
  'Your API Key — copy it now!': '你的 API 密钥 —— 请立即复制！',
  'This key will not be shown again. Store it somewhere safe.': '此密钥不会再次显示，请妥善保存。',
  'Name is required.': '请输入名称。',
  'e.g. CLI, CI/CD, Web': '例如：CLI、CI/CD、Web',
  'Join': '加入',
  'Joining...': '加入中…',
  'Public knowledge spaces you can browse and join.': '可以浏览并加入的公开知识空间。',
  'No public spaces yet.': '暂无公开空间。',

  // Login
  'Connect what people and AI agents know.': '连接人与 AI 智能体的知识。',
  'CoWiki · a collaborative LLM wiki': 'CoWiki · 人与 AI 协作的知识库',
  'Continue with GitHub': '使用 GitHub 继续',
  'Waiting for GitHub…': '正在等待 GitHub 授权…',
  'Continue locally': '本地模式继续',
  'Finish signing in in your browser.': '请在浏览器中完成登录。',
  'Sign-in was cancelled. Try again when ready.': '已取消登录，准备好后可以重试。',
  'Sign-in timed out before GitHub responded. Try again.': '等待 GitHub 响应超时，请重试。',
  'Could not open your browser for GitHub sign-in. Try again.': '无法打开浏览器完成 GitHub 登录，请重试。',
  'Could not sign in with GitHub. Try again.': 'GitHub 登录失败，请重试。',
  // Counts and dates in lists and bylines
  'Edited {date}': '编辑于 {date}',
  '{count} file': '{count} 个文件',
  '{count} files': '{count} 个文件',
  '{count} file changed': '{count} 个文件有改动',
  '{count} files changed': '{count} 个文件有改动',
  'Auto checkpoint': '自动快照',
} as const;

export type TranslationKey = keyof typeof ZH_CN;

function interpolate(template: string, vars?: Record<string, string | number>): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) => (
    Object.prototype.hasOwnProperty.call(vars, name) ? String(vars[name]) : match
  ));
}

/** Translate a known UI copy string with optional `{name}` interpolation. */
export function translate(locale: Locale, key: TranslationKey, vars?: Record<string, string | number>): string {
  const template = locale === 'zh-CN' ? ZH_CN[key] : key;
  return interpolate(template, vars);
}

/** Translate a dynamic string (e.g. plugin metadata) when a Chinese entry exists; otherwise return it as-is. */
export function translateLabel(locale: Locale, raw: string): string {
  if (locale !== 'zh-CN') return raw;
  return Object.prototype.hasOwnProperty.call(ZH_CN, raw)
    ? ZH_CN[raw as TranslationKey]
    : raw;
}

/** Browser Intl locale tag for date/time formatting. */
export function intlLocale(locale: Locale): string {
  return locale === 'zh-CN' ? 'zh-CN' : 'en-US';
}
