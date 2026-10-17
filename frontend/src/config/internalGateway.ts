export const INTERNAL_GATEWAY = true
export const COMMERCIAL_MODULES = new Set(['payment', 'payments', 'referral', 'billing', 'wallet', 'redeem_codes'])
export function isCommercialPage(path: string): boolean {
  return /^\/(?:dashboard\/(?:wallet|billing|referral)|admin\/(?:wallets|billing-plans|payment-gateways|referrals))(?:\/|$)/.test(path)
}
