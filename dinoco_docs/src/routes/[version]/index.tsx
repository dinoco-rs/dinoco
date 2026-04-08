import React, { useMemo } from 'react';
import { useRouter } from 'tuono-router';

import RouteRedirect from '../../components/RouteRedirect';
import { getDefaultVersionName, getFirstDocsPath, parseDocsPath } from '../../jsons/versions';
import { useDocs } from '../../hooks/useDocs';

const VersionIndex: React.FC = () => {
	const locale = useDocs(state => state.locale);
	const router = useRouter();

	const redirectPath = useMemo(() => {
		const params = parseDocsPath(router.pathname);

		return getFirstDocsPath(params.versionName ?? getDefaultVersionName(), locale);
	}, [locale, router.pathname]);

	return <RouteRedirect to={redirectPath} />;
};

export default VersionIndex;
