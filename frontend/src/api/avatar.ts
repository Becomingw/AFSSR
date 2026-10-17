import apiClient from './client'

/** Largest picture the server accepts; the console re-encodes to far less before uploading. */
export const MAX_AVATAR_BYTES = 256 * 1024

/**
 * URL of a user's picture. The version is part of the URL, so a replaced picture is a new URL and
 * the old one can stay cached forever. Without a version there is no picture to show.
 */
export function avatarSrc(userId?: string | null, version?: number | null): string | null {
  return userId && version ? `/api/avatars/${encodeURIComponent(userId)}?v=${version}` : null
}

export interface UploadedAvatar { version: number, url: string }

export async function uploadAvatar(picture: Blob): Promise<UploadedAvatar> {
  const response = await apiClient.put<UploadedAvatar>('/api/avatars/me', picture, {
    headers: { 'Content-Type': picture.type || 'application/octet-stream' },
    // The body is a binary picture, not JSON: send it exactly as it is.
    transformRequest: [(data: unknown) => data],
  })
  return response.data
}

export async function removeAvatar(): Promise<boolean> {
  return (await apiClient.delete<{ removed: boolean }>('/api/avatars/me')).data.removed
}
