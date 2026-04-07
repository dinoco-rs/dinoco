import React, { useMemo } from 'react';
import { useRouter } from 'tuono-router';

import RouteRedirect from '../../../components/RouteRedirect';
import { getDefaultVersionName, getFirstDocsPath, parseDocsPath, resolveDocsPath } from '../../../jsons/versions';
import { useDocs } from '../../../hooks/useDocs';

const GroupIndex: React.FC = () => {
	const locale = useDocs(state => state.locale);
	const router = useRouter();

	const redirectPath = useMemo(() => {
		const params = parseDocsPath(router.pathname);
		const resolved = resolveDocsPath({
			versionName: params.versionName ?? getDefaultVersionName(),
			groupShortName: params.groupShortName,
			locale,
		});

		return resolved?.path ?? getFirstDocsPath(params.versionName ?? getDefaultVersionName(), locale);
	}, [locale, router.pathname]);

	return <RouteRedirect to={redirectPath} />;
};

export default GroupIndex;
