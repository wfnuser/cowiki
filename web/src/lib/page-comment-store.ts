import type { MemberInfo, PageComment, PageCommentsResponse } from '../api';
import * as localApi from '../local-api';
import type { CloudClient, CloudComment } from '../cloud/client';

export interface CommentMember extends Pick<MemberInfo, 'id' | 'name'> {
  mention: string;
}

export interface PageCommentStore {
  readonly scope: 'local' | 'cloud';
  readonly scopeLabel: string;
  readonly currentUserId: string;
  readonly currentUserName: string;
  list(pagePath: string): Promise<PageCommentsResponse>;
  listMembers(): Promise<CommentMember[]>;
  create(input: {
    pagePath: string;
    body: string;
    source?: string;
    startLine?: number;
    endLine?: number;
    parentId?: string;
  }): Promise<PageComment>;
  setResolved(commentId: string, resolved: boolean): Promise<PageComment>;
  delete(commentId: string): Promise<void>;
}

export function localPageCommentStore(spaceSlug: string): PageCommentStore {
  const currentUserId = 'local';
  const currentUserName = 'You';
  return {
    scope: 'local',
    scopeLabel: 'Local only',
    currentUserId,
    currentUserName,
    list: (pagePath) => localApi.listPageComments(spaceSlug, pagePath),
    async listMembers() {
      const members = await localApi.listCommentMembers(spaceSlug);
      const normalized = members.map((member) => ({ ...member, mention: member.name }));
      return normalized.some((member) => member.id === currentUserId)
        ? normalized
        : [{ id: currentUserId, name: currentUserName, mention: currentUserName }, ...normalized];
    },
    create: (input) => localApi.createPageComment(spaceSlug, {
      pageSlug: input.pagePath,
      userId: currentUserId,
      userName: currentUserName,
      body: input.body,
      source: input.source,
      startLine: input.startLine,
      endLine: input.endLine,
      parentId: input.parentId,
    }),
    setResolved: (commentId, resolved) => localApi.setPageCommentResolved(spaceSlug, commentId, resolved),
    delete: (commentId) => localApi.deletePageComment(spaceSlug, commentId, currentUserId),
  };
}

export function cloudPageCommentStore(
  client: CloudClient,
  spaceId: string,
  currentUserId: string,
  currentUserName: string,
): PageCommentStore {
  return {
    scope: 'cloud',
    scopeLabel: 'Cloud shared',
    currentUserId,
    currentUserName,
    async list(pagePath) {
      const response = await client.listComments(spaceId, pagePath);
      return {
        comments: response.comments.map((comment) => cloudComment(comment, spaceId)),
        snapshots: response.snapshots.map((snapshot) => ({
          content_hash: snapshot.contentHash,
          source: snapshot.source,
        })),
      };
    },
    async listMembers() {
      return (await client.listMembers(spaceId)).map((member) => ({
        id: member.userId,
        name: member.displayName,
        mention: member.handle,
      }));
    },
    async create(input) {
      return cloudComment(await client.createComment(spaceId, {
        path: input.pagePath,
        body: input.body,
        source: input.source,
        startLine: input.startLine,
        endLine: input.endLine,
        parentId: input.parentId,
      }), spaceId);
    },
    async setResolved(commentId, resolved) {
      return cloudComment(await client.setCommentResolved(spaceId, commentId, resolved), spaceId);
    },
    delete: (commentId) => client.deleteComment(spaceId, commentId),
  };
}

function cloudComment(comment: CloudComment, workspaceSlug: string): PageComment {
  return {
    id: comment.id,
    workspace_slug: workspaceSlug,
    page_slug: comment.pagePath,
    user_id: comment.userId,
    content_hash: comment.contentHash,
    start_line: comment.startLine,
    end_line: comment.endLine,
    body: comment.body,
    parent_id: comment.parentId,
    resolved: comment.resolved,
    created_at: comment.createdAt,
    updated_at: comment.updatedAt,
  };
}
