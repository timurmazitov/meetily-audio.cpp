'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { MessagesSquare } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';

export default function FeedbackLink({ isCollapsed }: { isCollapsed: boolean }) {
  const { t } = useTranslation();
  const pathname = usePathname();
  const isActive = pathname === '/feedback';
  const link = (
    <Link
      href="/feedback"
      replace={pathname === '/settings'}
      aria-label={t('nav.feedback')}
      aria-current={isActive ? 'page' : undefined}
      className={`flex items-center justify-center rounded-lg transition-colors active:bg-gray-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-500 focus-visible:ring-offset-2 ${
        isCollapsed
          ? 'p-2'
          : 'w-full px-3 py-1.5 mt-1 mb-2 text-sm font-medium shadow-sm'
      } ${
        isCollapsed
          ? `text-gray-600 ${isActive ? 'bg-gray-100' : 'hover:bg-gray-100'}`
          : 'bg-gray-200 text-gray-700 hover:bg-gray-300'
      }`}
    >
      <MessagesSquare aria-hidden="true" className={isCollapsed ? 'w-5 h-5' : 'w-4 h-4'} />
      {!isCollapsed && <span className="ml-2">{t('nav.feedback')}</span>}
    </Link>
  );

  if (!isCollapsed) return link;

  return (
    <Tooltip>
      <TooltipTrigger asChild>{link}</TooltipTrigger>
      <TooltipContent side="right">{t('nav.feedback')}</TooltipContent>
    </Tooltip>
  );
}