/* ========================================
   BREADCRUMB - TYPES
   ======================================== */

import type { ComponentPropsWithoutRef, HTMLAttributes } from 'react';

export interface BreadcrumbProps extends ComponentPropsWithoutRef<'nav'> {
    separator?: React.ReactNode;
}

export interface BreadcrumbListProps extends ComponentPropsWithoutRef<'ol'> {}

export interface BreadcrumbItemProps extends HTMLAttributes<HTMLLIElement> {}

export interface BreadcrumbLinkProps extends ComponentPropsWithoutRef<'a'> {
    asChild?: boolean;
}

export interface BreadcrumbPageProps extends HTMLAttributes<HTMLSpanElement> {}